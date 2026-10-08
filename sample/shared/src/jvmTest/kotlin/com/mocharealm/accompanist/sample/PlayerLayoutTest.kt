package com.mocharealm.accompanist.sample

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.material3.Text
import androidx.compose.runtime.*
import androidx.compose.runtime.snapshots.Snapshot
import androidx.compose.ui.InternalComposeUiApi
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.graphics.*
import androidx.compose.ui.layout.boundsInRoot
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.platform.FrameRecomposer
import androidx.compose.ui.scene.CanvasLayersComposeScene
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.dp
import com.mocharealm.accompanist.sample.ui.composable.player.*
import com.mocharealm.accompanist.sample.ui.theme.AccompanistTheme
import java.io.File
import kotlin.test.*
import kotlinx.coroutines.Dispatchers

@OptIn(InternalComposeUiApi::class)
class PlayerLayoutTest {
    @Test
    fun resizingKeepsLyricsVisibleAndSwitchesArtworkPlacement() {
        val recomposer = FrameRecomposer(Dispatchers.Unconfined)
        val scene = CanvasLayersComposeScene(frameRecomposer = recomposer, size = IntSize(400, 800))
        var hasArtwork by mutableStateOf(true)
        var artwork: Rect? = null
        var lyrics: Rect? = null
        var controls: Rect? = null
        var nanos = 0L
        fun settle() {
            repeat(90) {
                Snapshot.sendApplyNotifications()
                nanos += 16_666_667L
                recomposer.performFrame(nanos)
                scene.measureAndLayout()
            }
        }
        fun preview(name: String) {
            val size = scene.size!!
            val bitmap = ImageBitmap(size.width, size.height)
            scene.draw(Canvas(bitmap))
            val destination = File("build/player-layout-preview/$name.png")
            destination.parentFile.mkdirs()
            org.jetbrains.skia.Image.makeFromBitmap(bitmap.asSkiaBitmap()).use { image ->
                image.encodeToData()!!.use { destination.writeBytes(it.bytes) }
            }
        }
        try {
            scene.setContent {
                AccompanistTheme(darkTheme = true) {
                    PlayerLayout(
                        title = "A long song title that should leave room for controls",
                        artist = "Sample artist",
                        hasArtwork = hasArtwork,
                        modifier = Modifier.background(Color(0xFF443D55)),
                        artwork = { modifier ->
                            Box(modifier.onGloballyPositioned { artwork = it.boundsInRoot() }
                                .background(Brush.linearGradient(listOf(Color(0xFFAE888A), Color(0xFF4A526B)))))
                        },
                        controls = {
                            PlayerControls({}, true, true, {}, {},
                                Modifier.onGloballyPositioned { controls = it.boundsInRoot() })
                        },
                        lyrics = { modifier, _, _ ->
                            val typography = rememberPlayerLyricsTypography()
                            Column(modifier.onGloballyPositioned { lyrics = it.boundsInRoot() }
                                .padding(top = 64.dp)) {
                                Text("A line of lyrics", style = typography.normal, color = Color.White)
                                Text("The next line", style = typography.normal, color = Color.White.copy(.4f))
                            }
                        },
                    )
                }
            }
            settle()
            val phoneArtwork = assertNotNull(artwork)
            val phoneLyrics = assertNotNull(lyrics)
            assertTrue(phoneArtwork.bottom <= phoneLyrics.top, "Phone artwork belongs above the lyrics")
            assertEquals(72f, phoneArtwork.width)
            assertEquals(72f, phoneArtwork.height)
            assertEquals(800f, phoneLyrics.bottom, "Lyrics use the full remaining height")
            assertTrue(assertNotNull(controls).right <= 400f)
            preview("phone")

            scene.size = IntSize(1200, 800)
            settle()
            val wideArtwork = assertNotNull(artwork)
            val wideLyrics = assertNotNull(lyrics)
            assertTrue(wideArtwork.right < wideLyrics.left, "Wide artwork belongs beside the lyrics")
            assertTrue(wideArtwork.width > 72f)
            assertEquals(wideArtwork.width, wideArtwork.height)
            assertEquals(800f, wideLyrics.bottom)
            assertTrue(assertNotNull(controls).right < wideLyrics.left)
            preview("wide")

            hasArtwork = false
            artwork = null
            settle()
            assertNotNull(artwork, "Wide layout keeps its fallback cover")
            scene.size = IntSize(400, 800)
            artwork = null
            settle()
            assertNull(artwork, "Phone layout omits an unavailable cover")
            assertEquals(800f, assertNotNull(lyrics).bottom)
        } finally {
            scene.close()
            recomposer.close()
        }
    }
}
