package com.mocharealm.accompanist.lyrics.ui.composable.lyrics

import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.snapshots.Snapshot
import androidx.compose.ui.InternalComposeUiApi
import androidx.compose.ui.graphics.Canvas
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.platform.FrameRecomposer
import androidx.compose.ui.scene.CanvasLayersComposeScene
import androidx.compose.ui.unit.IntSize
import com.mocharealm.accompanist.lyrics.core.model.SyncedLyrics
import com.mocharealm.accompanist.lyrics.core.model.karaoke.KaraokeAlignment
import com.mocharealm.accompanist.lyrics.core.model.karaoke.KaraokeLine
import com.mocharealm.accompanist.lyrics.core.model.karaoke.KaraokeSyllable
import com.mocharealm.accompanist.lyrics.ui.composable.list.LyricsLazyListState
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertTrue

@OptIn(InternalComposeUiApi::class)
class AccompanimentFollowTest {
    @Test fun interludeUsesDisplayedIndexAfterEmbeddedAccompanimentIsRemoved() {
        val acc = KaraokeLine.AccompanimentKaraokeLine(
            listOf(KaraokeSyllable("echo", 0, 500)), null,
            KaraokeAlignment.Start, 0, 500,
        )
        val main = KaraokeLine.MainKaraokeLine(
            listOf(KaraokeSyllable("main", 0, 1000)), null,
            KaraokeAlignment.Start, 0, 1000, accompanimentLines = listOf(acc),
        )
        val next = KaraokeLine.MainKaraokeLine(
            listOf(KaraokeSyllable("next", 10000, 12000)), null,
            KaraokeAlignment.Start, 10000, 12000,
        )
        val lyrics = SyncedLyrics(listOf(main, acc, next))
        val state = LyricsLazyListState()
        val recomposer = FrameRecomposer(kotlinx.coroutines.Dispatchers.Unconfined)
        val scene = CanvasLayersComposeScene(recomposer, size = IntSize(400, 900))
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
            scene.setContent { KaraokeLyricsView(state, lyrics, { 2000 }, {}, {}) }
            val deadline = System.nanoTime() + 10_000_000_000L
            while (state.items.isEmpty() && System.nanoTime() < deadline) {
                frame()
                Thread.sleep(5)
            }
            repeat(3) { frame() }
            assertEquals(2, state.items.size)
            assertEquals(1, state.interludeItemIndex,
                "Source line 2 is displayed item 1 after the embedded vocal is removed")
        } finally {
            scene.close()
            recomposer.close()
        }
    }

    @Test fun trailingVocalsCollapseWithoutPushingNextLeadingVocalsPastAnchor() {
        fun accompaniment(start: Int, end: Int) = KaraokeLine.AccompanimentKaraokeLine(
            listOf(KaraokeSyllable("A long accompaniment that wraps across rows", start, end)),
            "Accompaniment translation", KaraokeAlignment.Start, start, end,
        )
        fun main(start: Int, end: Int, textStart: Int = start, acc: KaraokeLine.AccompanimentKaraokeLine? = null) =
            KaraokeLine.MainKaraokeLine(
                listOf(KaraokeSyllable("A main lyric spanning multiple rows", textStart, end)),
                "Main translation", KaraokeAlignment.Start, start, end,
                accompanimentLines = acc?.let { listOf(it) },
            )
        // Timing from the reported 2:45 transition, with generated text. The next
        // leading vocal starts while the previous trailing vocal is still visible.
        val lyrics = SyncedLyrics(listOf(
            main(150000, 156358),
            main(156358, 161745, acc = accompaniment(158421, 161745)),
            main(161745, 163729),
            main(163729, 167749, acc = accompaniment(164327, 167302)),
            main(166632, 172466, textStart = 168000, acc = accompaniment(166632, 171364)),
            main(170701, 173573),
            main(172480, 177210),
            main(177210, 185000),
        ))
        val time = mutableIntStateOf(160000)
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
            scene.setContent { KaraokeLyricsView(listState=state, lyrics=lyrics, currentPosition={time.intValue}, onLineClicked={}, onLinePressed={}) }
            val deadline = System.nanoTime() + 20_000_000_000L
            while (state.items.isEmpty() && System.nanoTime() < deadline) { frame(); Thread.sleep(5) }
            assertTrue(state.items.isNotEmpty())
            repeat(240) { frame() }
            repeat(750) {
                time.intValue += 16
                frame()
                if (time.intValue in 167750..170000) {
                    val aggregateTop = state.heights.top(4) - state.position + state.chain.offset(4)
                    assertTrue(aggregateTop >= -1.0, "t=${time.intValue}: leading accompaniment crossed anchor: $aggregateTop")
                    if (time.intValue >= 169000) assertTrue(kotlin.math.abs(aggregateTop) <= 1.0, "Aggregate must settle at anchor: $aggregateTop")
                }
            }
        } finally { scene.close(); recomposer.close() }
    }
}
