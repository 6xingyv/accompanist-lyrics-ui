package com.mocharealm.accompanist.sample

import androidx.compose.runtime.*
import androidx.compose.runtime.snapshots.Snapshot
import androidx.compose.ui.ImageComposeScene
import androidx.compose.ui.semantics.*
import com.mocharealm.accompanist.sample.ui.composable.player.SongSelectionDialogContent
import com.mocharealm.accompanist.sample.ui.theme.AccompanistTheme
import java.io.File
import kotlin.test.*

class SongSelectionDialogTest {
    @Test
    fun filesAreSelectedInsideTheSharedDialogAndOnlyPlayConfirmsThem() {
        var selected by mutableStateOf(false)
        var ready by mutableStateOf(false)
        var importing by mutableStateOf(false)
        var error by mutableStateOf<String?>(null)
        var plays = 0
        var lyricsPicks = 0
        var translationPicks = 0
        var dismisses = 0
        val scene = ImageComposeScene(420, 640) {
            AccompanistTheme(darkTheme = true) {
                SongSelectionDialogContent(
                    audioName = if (selected) "Song.m4a" else "Select audio",
                    lyricsName = "Select lyrics (optional)", translationName = "Select translation (optional)",
                    audioSelected = selected, isReady = ready, isImporting = importing, error = error,
                    onSelectAudio = { selected = true },
                    onSelectLyrics = { lyricsPicks++ }, onSelectTranslation = { translationPicks++ },
                    onPlay = { plays++ }, onDismissRequest = { dismisses++ },
                )
            }
        }
        var nanos = 0L
        fun settle(name: String) {
            repeat(8) {
                Snapshot.sendApplyNotifications()
                nanos += 16_666_667
                scene.render(nanos).close()
            }
            scene.render(nanos).use { image ->
                File("build/song-selection-preview/$name.png").also { file ->
                    file.parentFile.mkdirs()
                    image.encodeToData()!!.use { file.writeBytes(it.bytes) }
                }
            }
        }
        fun descendants(node: SemanticsNode): List<SemanticsNode> =
            listOf(node) + node.children.flatMap(::descendants)
        fun node(label: String, button: Boolean = true): SemanticsNode =
            scene.semanticsOwners.flatMap { descendants(it.rootSemanticsNode) }.first {
                it.config.getOrNull(SemanticsProperties.Text)?.any { text -> text.text == label } == true &&
                    (!button || SemanticsActions.OnClick in it.config)
            }
        fun click(label: String) {
            assertTrue(assertNotNull(node(label).config[SemanticsActions.OnClick].action).invoke())
        }
        try {
            settle("initial")
            node("Open local files", button = false)
            assertTrue(SemanticsProperties.Disabled in node("Play").config)
            click("Select audio")
            settle("selected")
            assertTrue(selected)
            assertEquals(0, plays, "Picking a file must not start playback")
            assertTrue(SemanticsProperties.Disabled in node("Play").config)
            click("Select lyrics (optional)")
            click("Select translation (optional)")
            assertEquals(1, lyricsPicks)
            assertEquals(1, translationPicks)
            ready = true
            settle("ready")
            assertFalse(SemanticsProperties.Disabled in node("Play").config)
            click("Play")
            assertEquals(1, plays)
            importing = true
            settle("opening")
            assertTrue(SemanticsProperties.Disabled in node("Opening…").config)
            assertTrue(SemanticsProperties.Disabled in node("Song.m4a").config)
            importing = false
            error = "Unable to read timed lyrics."
            settle("error")
            node("Unable to read timed lyrics.", button = false)
            assertFalse(SemanticsProperties.Disabled in node("Play").config)
            click("Cancel")
            assertEquals(1, dismisses)
        } finally { scene.close() }
    }
}
