package com.mocharealm.accompanist.lyrics.ui.composable.lyrics

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.runtime.*
import androidx.compose.runtime.snapshots.Snapshot
import androidx.compose.ui.InternalComposeUiApi
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.*
import androidx.compose.ui.platform.FrameRecomposer
import androidx.compose.ui.scene.CanvasLayersComposeScene
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.dp
import com.mocharealm.accompanist.lyrics.core.model.SyncedLyrics
import com.mocharealm.accompanist.lyrics.core.model.synced.SyncedLine
import com.mocharealm.accompanist.lyrics.ui.composable.list.LyricsLazyListState
import com.mocharealm.accompanist.lyrics.ui.profile.FallbackProfile
import kotlin.test.*
import kotlinx.coroutines.Dispatchers

@OptIn(InternalComposeUiApi::class)
class LyricsAnchorTest {
    @Test
    fun fixedAndFractionAnchorsPositionInitialFocusAndTrackViewHeight() {
        val lyrics = SyncedLyrics(List(8) { index ->
            val start = index * 10000
            SyncedLine("Hello", null, start, start + 10000)
        })
        val state = LyricsLazyListState()
        var anchor by mutableStateOf<LyricsAnchor>(LyricsAnchor.Fixed(32.dp))
        var height by mutableStateOf(600.dp)
        val time = mutableIntStateOf(5000)
        val recomposer = FrameRecomposer(Dispatchers.Unconfined)
        val scene = CanvasLayersComposeScene(recomposer, size = IntSize(400, 1000))
        val bitmap = ImageBitmap(400, 1000)
        val canvas = Canvas(bitmap)
        val clear = Paint().apply { blendMode = BlendMode.Clear }
        var nanos = 0L
        fun frame() {
            Snapshot.sendApplyNotifications()
            nanos += 16_666_667L
            recomposer.performFrame(nanos)
            scene.measureAndLayout()
            canvas.drawRect(0f, 0f, 400f, 1000f, clear)
            scene.draw(canvas)
        }
        fun activeInkTop(): Int {
            repeat(150) { frame(); Thread.sleep(2) }
            val pixels = IntArray(400 * 1000)
            bitmap.readPixels(pixels)
            // Inactive items use 40% opacity; the focused lyric is brighter.
            val first = pixels.indexOfFirst { (it ushr 24) > 240 }
            assertTrue(first >= 0, "Focused lyric must be visibly drawn")
            return first / 400
        }
        try {
            scene.setContent {
                Box(Modifier.fillMaxWidth().height(height)) {
                    // Verify initial focus at each position; anchor/height changes retain the view.
                    key(time.intValue) {
                        KaraokeLyricsView(
                            state, lyrics, { time.intValue }, {}, {},
                            translationTextStyle = androidx.compose.ui.text.TextStyle(),
                            anchor = anchor,
                            topFade = LyricsFade.Fixed(0.dp),
                            bottomFade = LyricsFade.Fixed(0.dp),
                            renderProfiles = listOf(FallbackProfile),
                            useBlurEffect = false,
                            scrollChain = null,
                        )
                    }
                }
            }
            val deadline = System.nanoTime() + 10_000_000_000L
            while (!state.ready && System.nanoTime() < deadline) { frame(); Thread.sleep(5) }
            assertTrue(state.ready)
            val inkOffset = activeInkTop() - 32
            for (index in listOf(0, 3, 7)) {
                time.intValue = index * 10000 + 5000
                anchor = LyricsAnchor.Fixed(128.dp)
                assertEquals(128 + inkOffset, activeInkTop(), "Fixed anchor, item=$index")
                anchor = LyricsAnchor.Fraction(0.4f)
                assertEquals(240 + inkOffset, activeInkTop(), "40% of lyrics view, item=$index")
                anchor = LyricsAnchor.Fraction(0.5f)
                assertEquals(300 + inkOffset, activeInkTop(), "Center anchor, item=$index")
            }
            anchor = LyricsAnchor.Fraction(0.4f)
            height = 800.dp
            assertEquals(320 + inkOffset, activeInkTop(), "Resize without a lyric boundary change")
            anchor = LyricsAnchor.Fixed(128.dp)
            assertEquals(128 + inkOffset, activeInkTop(), "Fixed anchor is independent of height")
        } finally {
            scene.close()
            recomposer.close()
        }
    }
}

