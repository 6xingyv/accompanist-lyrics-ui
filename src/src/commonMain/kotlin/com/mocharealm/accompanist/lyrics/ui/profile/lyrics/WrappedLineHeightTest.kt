package com.mocharealm.accompanist.lyrics.ui.composable.lyrics

import com.mocharealm.accompanist.lyrics.ui.internal.test.TestSceneDispatcher

import androidx.compose.material3.LocalTextStyle
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.snapshots.Snapshot
import androidx.compose.ui.InternalComposeUiApi
import androidx.compose.ui.graphics.Canvas
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.platform.FrameRecomposer
import androidx.compose.ui.scene.CanvasLayersComposeScene
import androidx.compose.ui.text.TextMeasurer
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.sp
import com.mocharealm.accompanist.lyrics.core.model.SyncedLyrics
import com.mocharealm.accompanist.lyrics.core.model.karaoke.*
import com.mocharealm.accompanist.lyrics.ui.composable.list.LyricsLazyListState
import com.mocharealm.accompanist.lyrics.ui.profile.ProfileTextUnit
import com.mocharealm.accompanist.lyrics.ui.profile.DefaultLyricsProfile
import java.util.concurrent.atomic.AtomicReference
import kotlin.test.*

@OptIn(InternalComposeUiApi::class)
class WrappedLineHeightTest {
    @Test
    fun wrappedSyllableUsesLyricFontHeightInsteadOfAmbientBodyLineHeight() {
        val wrapped = AtomicReference<List<ProfileTextUnit>?>()
        val profile =
            object : DefaultLyricsProfile() {
                override fun wrap(
                    units: List<ProfileTextUnit>,
                    measurer: TextMeasurer,
                    maxWidth: Float,
                ): List<List<ProfileTextUnit>> =
                    super.wrap(units, measurer, maxWidth).also { wrapped.set(it.flatten()) }
            }
        val line =
            KaraokeLine.MainKaraokeLine(
                listOf(KaraokeSyllable("office".repeat(15), 0, 10000)),
                null,
                KaraokeAlignment.Start,
                0,
                10000,
            )
        val lyrics = SyncedLyrics(listOf(line))
        val state = LyricsLazyListState()
        val dispatcher = TestSceneDispatcher()
        val recomposer = FrameRecomposer(dispatcher)
        val scene = CanvasLayersComposeScene(recomposer, size = IntSize(320, 900))
        val canvas = Canvas(ImageBitmap(320, 900))
        var nanos = 0L
        fun frame() {
            dispatcher.runCurrent()
            Snapshot.sendApplyNotifications()
            nanos += 16_666_667L
            recomposer.performFrame(nanos)
            scene.measureAndLayout()
            scene.draw(canvas)
        }
        try {
            scene.setContent {
                CompositionLocalProvider(
                    LocalTextStyle provides TextStyle(fontSize = 16.sp, lineHeight = 24.sp)
                ) {
                    KaraokeLyricsView(
                        state,
                        lyrics,
                        { 5000 },
                        {},
                        {},
                        translationTextStyle = androidx.compose.ui.text.TextStyle(),
                        renderProfiles = listOf(profile),
                    )
                }
            }
            val deadline = System.nanoTime() + 10_000_000_000L
            while ((!state.ready || wrapped.get() == null) && System.nanoTime() < deadline) {
                frame()
                Thread.sleep(5)
            }
            repeat(120) { frame() }
            val units = assertNotNull(wrapped.get())
            assertTrue(units.size > 1, "Fixture must wrap inside a single syllable")
            assertTrue(
                units.all { it.height >= 34f },
                "34sp lyrics inherited the ambient 24sp line height: ${units.map { it.height }}",
            )
            assertTrue(state.heights.height(0) >= units.sumOf { it.height.toDouble() }.toInt())
        } finally {
            scene.close()
            recomposer.close()
            dispatcher.runCurrent()
        }
    }
}
