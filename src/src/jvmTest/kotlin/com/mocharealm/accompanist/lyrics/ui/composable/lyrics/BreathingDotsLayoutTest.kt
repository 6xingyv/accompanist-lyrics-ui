package com.mocharealm.accompanist.lyrics.ui.composable.lyrics

import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.snapshots.Snapshot
import androidx.compose.ui.InternalComposeUiApi
import androidx.compose.ui.graphics.Canvas
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.platform.FrameRecomposer
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.scene.CanvasLayersComposeScene
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.rememberTextMeasurer
import androidx.compose.ui.text.style.TextMotion
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.mocharealm.accompanist.lyrics.core.model.SyncedLyrics
import com.mocharealm.accompanist.lyrics.core.model.karaoke.KaraokeAlignment
import com.mocharealm.accompanist.lyrics.core.model.karaoke.KaraokeLine
import com.mocharealm.accompanist.lyrics.core.model.karaoke.KaraokeSyllable
import com.mocharealm.accompanist.lyrics.ui.composable.list.LyricsLazyListState
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertTrue
import kotlinx.coroutines.Dispatchers

@OptIn(InternalComposeUiApi::class)
class BreathingDotsLayoutTest {
    @Test
    fun introAndInterludeReserveOneTextRowAndCollapseIncludingSpacing() {
        for (fontScale in listOf(1f, 1.6f)) {
            val density = Density(1f, fontScale)
            val style = TextStyle(fontSize = 32.sp, lineHeight = 72.sp, textMotion = TextMotion.Animated)
            val lyrics = SyncedLyrics(listOf(10000, 20000).map { start ->
                KaraokeLine.MainKaraokeLine(
                    listOf(KaraokeSyllable("Hello", start, start + 2000, phonetic = "hello")),
                    "Translation", KaraokeAlignment.Start, start, start + 2000,
                )
            })
            val time = mutableIntStateOf(5000)
            val state = LyricsLazyListState()
            val recomposer = FrameRecomposer(Dispatchers.Unconfined)
            val scene = CanvasLayersComposeScene(recomposer, size = IntSize(600, 1400))
            val canvas = Canvas(ImageBitmap(600, 1400))
            var expectedRowHeight = 0
            var nanos = 0L
            fun frame() {
                Snapshot.sendApplyNotifications()
                nanos += 16_666_667L
                recomposer.performFrame(nanos)
                scene.measureAndLayout()
                scene.draw(canvas)
            }
            fun settleAt(position: Int) {
                time.intValue = position
                repeat(180) { frame() }
            }
            try {
                scene.setContent {
                    CompositionLocalProvider(LocalDensity provides density) {
                        expectedRowHeight = rememberTextMeasurer()
                            .measure("Hello", style, softWrap = false).size.height
                        KaraokeLyricsView(
                            state, lyrics, { time.intValue }, {}, {},
                            normalLineTextStyle = style,
                            translationTextStyle = androidx.compose.ui.text.TextStyle(),
                            itemSpacing = 24.dp,
                            scrollChain = null,
                        )
                    }
                }
                val deadline = System.nanoTime() + 10_000_000_000L
                while ((!state.ready || state.items.size != 2) && System.nanoTime() < deadline) {
                    frame()
                    Thread.sleep(5)
                }
                assertTrue(state.ready)
                settleAt(5000)
                val introHeight = state.heights.height(0)
                settleAt(11000)
                val lyricHeight = state.heights.height(0)
                assertEquals(expectedRowHeight + 24, introHeight - lyricHeight,
                    "Intro must occupy one uncaptioned row plus spacing, fontScale=$fontScale")
                assertTrue(lyricHeight > expectedRowHeight, "Fixture must include captions")

                settleAt(15000)
                val interludeHeight = state.heights.height(1)
                settleAt(21000)
                assertEquals(expectedRowHeight + 24, interludeHeight - state.heights.height(1),
                    "Interlude must use the same row height and leave no residual gap")
                settleAt(15000)
                assertEquals(interludeHeight, state.heights.height(1),
                    "Seeking back must restore the complete interlude slot")
            } finally {
                scene.close()
                recomposer.close()
            }
        }
    }
}
