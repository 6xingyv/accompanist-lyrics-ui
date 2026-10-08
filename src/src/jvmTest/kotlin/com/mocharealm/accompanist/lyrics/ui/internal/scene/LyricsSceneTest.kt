package com.mocharealm.accompanist.lyrics.ui.internal.scene

import com.mocharealm.accompanist.lyrics.ui.internal.test.TestSceneDispatcher

import androidx.compose.runtime.*
import androidx.compose.runtime.snapshots.Snapshot
import androidx.compose.ui.InternalComposeUiApi
import androidx.compose.ui.graphics.Canvas
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.platform.FrameRecomposer
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalLayoutDirection
import androidx.compose.ui.scene.CanvasLayersComposeScene
import androidx.compose.ui.text.TextMeasurer
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.rememberTextMeasurer
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.sp
import com.mocharealm.accompanist.lyrics.core.model.SyncedLyrics
import com.mocharealm.accompanist.lyrics.core.model.karaoke.KaraokeAlignment
import com.mocharealm.accompanist.lyrics.core.model.karaoke.KaraokeLine
import com.mocharealm.accompanist.lyrics.core.model.karaoke.KaraokeSyllable
import com.mocharealm.accompanist.lyrics.ui.internal.scene.rememberLyricsScene
import com.mocharealm.accompanist.lyrics.ui.profile.LatinProfile
import com.mocharealm.accompanist.lyrics.ui.profile.LyricsProfile
import com.mocharealm.accompanist.lyrics.ui.preparation.MeasuredLyricsLine
import com.mocharealm.accompanist.lyrics.ui.profile.ProfileTextUnit
import java.util.concurrent.atomic.AtomicInteger
import kotlin.test.*

@OptIn(InternalComposeUiApi::class)
class LyricsSceneTest {
    @Test
    fun colorKeepsLayoutAndPlaybackWhileWidthRebuildsGeometry() {
        val uiThread = Thread.currentThread()
        val preparations = AtomicInteger()
        val uiPreparations = AtomicInteger()
        val profile = object : LyricsProfile by LatinProfile {
            override fun prepare(
                line: MeasuredLyricsLine,
                group: List<KaraokeSyllable>,
                measurer: TextMeasurer,
                style: TextStyle,
            ): List<ProfileTextUnit> {
                preparations.incrementAndGet()
                if (Thread.currentThread() === uiThread) uiPreparations.incrementAndGet()
                return LatinProfile.prepare(line, group, measurer, style)
            }
        }
        val lyrics = SyncedLyrics(listOf(KaraokeLine.MainKaraokeLine(
            listOf(KaraokeSyllable("Singing across the sky", 0, 10000)), null,
            KaraokeAlignment.Start, 0, 10000,
        )))
        val color = mutableStateOf(Color.White)
        val width = mutableFloatStateOf(400f)
        val provider = mutableStateOf<() -> Int>({ 1000 })
        val dispatcher = TestSceneDispatcher()
        val recomposer = FrameRecomposer(dispatcher)
        val host = CanvasLayersComposeScene(recomposer, size = IntSize(500, 500))
        val canvas = Canvas(ImageBitmap(500, 500))
        var published: LyricsScene? = null
        var nanos = 0L
        fun frame() {
            dispatcher.runCurrent()
            Snapshot.sendApplyNotifications()
            nanos += 16_666_667L
            recomposer.performFrame(nanos)
            host.measureAndLayout()
            host.draw(canvas)
        }
        fun awaitScene(predicate: (LyricsScene) -> Boolean): LyricsScene {
            val deadline = System.nanoTime() + 10_000_000_000L
            while (System.nanoTime() < deadline) {
                frame()
                published?.let { if (predicate(it)) return it }
                Thread.sleep(5)
            }
            fail("Scene was not published in time")
        }
        try {
            host.setContent {
                val style = TextStyle(fontSize = 30.sp)
                val scene = rememberLyricsScene(
                    LyricsLayoutRequest(
                        lyrics, listOf(profile), rememberTextMeasurer(), style, style, style,
                        width.floatValue, LocalDensity.current, LocalLayoutDirection.current,
                        androidx.compose.ui.text.TextStyle(),
                    ),
                    color.value,
                    provider.value,
                )
                SideEffect { published = scene }
            }
            val white = awaitScene { it.resources.color == Color.White }
            val count = preparations.get()
            assertTrue(count > 0)
            color.value = Color.Red
            val red = awaitScene { it.resources.color == Color.Red }
            assertSame(white.lyrics, red.lyrics)
            assertSame(white.timeline, red.timeline)
            assertSame(white.items, red.items)
            assertNotSame(white.resources, red.resources)
            assertEquals(count, preparations.get(), "Color must not shape text again")

            provider.value = { 6500 }
            repeat(3) { frame() }
            val row = red.lyrics.rows.single()
            assertEquals(6500, red.timeline.state.row(row).time.intValue)
            assertSame(red, published)

            width.floatValue = 160f
            val narrow = awaitScene { it.lyrics !== red.lyrics }
            assertNotSame(red.timeline, narrow.timeline)
            assertNotSame(red.resources, narrow.resources)
            assertTrue(preparations.get() > count)
            assertTrue(narrow.lyrics.lines.single()!!.rows.size > red.lyrics.lines.single()!!.rows.size)
            assertEquals(0, uiPreparations.get(), "Layout preparation must run off composition")
        } finally {
            host.close()
            recomposer.close()
            dispatcher.runCurrent()
        }
    }
}
