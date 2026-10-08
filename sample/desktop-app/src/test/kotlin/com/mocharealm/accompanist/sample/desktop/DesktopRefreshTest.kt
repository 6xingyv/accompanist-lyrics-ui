package com.mocharealm.accompanist.sample.desktop

import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Rule
import org.junit.rules.TemporaryFolder
import java.nio.file.Files
import java.util.concurrent.atomic.AtomicInteger
import kotlin.test.*

class DesktopRefreshTest {
    @get:Rule val temporary = TemporaryFolder()

    private fun config(): DesktopConfig {
        val root = temporary.newFolder().toPath()
        val path = root.resolve("config.json")
        Files.writeString(path, """{"lyrics_dir":"lyrics"}""")
        return DesktopConfig.load(arrayOf("--config", path.toString()))
    }

    @Test
    fun rescanReconnectsAfterAnInFlightSnapshotAndReturnsToMediaFollowing() = runBlocking<Unit> {
        val entered = CompletableDeferred<Unit>()
        val release = CompletableDeferred<Unit>()
        val closed = CompletableDeferred<Unit>()
        val created = AtomicInteger()
        val controller = DesktopController(config(), sessionFactory = {
            val number = created.incrementAndGet()
            object : TestSession("Track $number") {
                override suspend fun snapshot(): MediaSnapshot {
                    if (number == 1) { entered.complete(Unit); release.await() }
                    return super.snapshot()
                }
                override fun close() { if (number == 1) closed.complete(Unit) }
            }
        })
        try {
            withTimeout(5000) { entered.await() }
            val preview = temporary.newFile("Preview.lrc").toPath()
            Files.writeString(preview, "[00:00.00]Preview\n[00:05.00]End")
            controller.openPreview(preview)
            withTimeout(5000) { controller.state.first { !it.following && !it.loading } }
            controller.rescanMediaSessions()
            assertFalse(closed.isCompleted, "A native operation must finish before disconnecting")
            release.complete(Unit)
            val state = withTimeout(5000) { controller.state.first { it.title == "Track 2" } }
            assertTrue(state.following)
            assertTrue(closed.isCompleted)
            assertEquals(2, created.get())
        } finally { release.complete(Unit); controller.close() }
    }

    @Test
    fun rescanLyricsFindsNewFilesAndReadsChangedContents() = runBlocking<Unit> {
        val config = config()
        val controller = DesktopController(config, sessionFactory = { TestSession("Song") })
        try {
            withTimeout(5000) { controller.state.first { it.message?.startsWith("No matching lyrics") == true } }
            val file = config.lyricsDir.resolve("Song.lrc")
            Files.writeString(file, "[00:00.00]First line\n[00:05.00]End")
            controller.reloadLyrics()
            val first = withTimeout(5000) { controller.state.first { it.lyrics != null } }.lyrics
            Files.writeString(file, "[00:00.00]Changed line\n[00:05.00]End")
            controller.reloadLyrics()
            val changed = withTimeout(5000) { controller.state.first { it.lyrics != null && it.lyrics != first } }
            assertEquals("Song.lrc", changed.fileName)
        } finally { controller.close() }
    }

    @Test
    fun reloadConfigurationRematchesUsingNewDirectoryAndPreservesWorkingConfigOnFailure() = runBlocking<Unit> {
        val config = config()
        val controller = DesktopController(config, sessionFactory = { TestSession("Song") })
        try {
            withTimeout(5000) { controller.state.first { it.title == "Song" && !it.loading } }
            val newDirectory = Files.createDirectories(config.configFile.parent.resolve("new-lyrics/nested"))
            Files.writeString(newDirectory.resolve("Song.lrc"), "[00:00.00]New directory\n[00:05.00]End")
            Files.writeString(config.configFile, """{"lyrics_dir":"new-lyrics","recursive":true,
                "window":{"width":700,"height":600,"min_width":400,"always_on_top":false}}""")
            controller.reloadConfiguration()
            val updated = withTimeout(5000) { controller.configuration.first { it.width == 700 } }
            assertEquals(600, updated.height)
            assertEquals(400, updated.minWidth)
            assertFalse(updated.alwaysOnTop)
            assertTrue(updated.recursive)
            withTimeout(5000) { controller.state.first { it.fileName == "Song.lrc" && !it.loading } }
            Files.writeString(config.configFile, "invalid JSON")
            controller.reloadConfiguration()
            withTimeout(5000) { controller.state.first { it.message != null && !it.loading } }
            assertEquals(updated, controller.config)
            assertNotNull(controller.state.value.lyrics)
            assertNotNull(controller.state.value.actionError)
        } finally { controller.close() }
    }

    private open class TestSession(private val title: String) : MediaSession {
        override suspend fun snapshot(): MediaSnapshot = MediaSnapshot(
            source = "test", trackId = title, title = title, artist = "Artist",
            position = 0, duration = 10000, playing = false, canSeek = true,
        )
        override suspend fun seek(snapshot: MediaSnapshot, position: Long) = true
        override suspend fun togglePlayback(snapshot: MediaSnapshot) = true
    }
}
