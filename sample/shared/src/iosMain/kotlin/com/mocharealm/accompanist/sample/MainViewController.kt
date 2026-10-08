@file:OptIn(kotlinx.cinterop.ExperimentalForeignApi::class)

package com.mocharealm.accompanist.sample

import androidx.compose.foundation.layout.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.toComposeImageBitmap
import androidx.compose.ui.window.ComposeUIViewController
import com.mocharealm.accompanist.lyrics.core.model.SyncedLyrics
import com.mocharealm.accompanist.lyrics.core.parser.AutoParser
import com.mocharealm.accompanist.lyrics.ui.composable.list.rememberLyricsLazyListState
import com.mocharealm.accompanist.sample.ui.composable.background.BackgroundVisualState
import com.mocharealm.accompanist.sample.ui.composable.player.*
import com.mocharealm.accompanist.sample.ui.theme.AccompanistTheme
import kotlinx.cinterop.addressOf
import kotlinx.cinterop.usePinned
import kotlinx.coroutines.*
import org.jetbrains.skia.Image as SkiaImage
import platform.Foundation.NSData
import platform.posix.memcpy
import platform.UIKit.UIViewController

/** Swift's AVAudioPlayer/CADisplayLink owns the clock; the shared view consumes its samples. */
class IosLyricsController {
    internal var lyrics by mutableStateOf<SyncedLyrics?>(null)
    internal val position = mutableIntStateOf(0)
    internal var translation by mutableStateOf(true)
    internal var pronunciation by mutableStateOf(true)
    internal var message by mutableStateOf("Open a timed lyrics file to begin")
    internal var title by mutableStateOf("Accompanist")
    internal var artist by mutableStateOf("Local preview")
    internal var background by mutableStateOf(BackgroundVisualState(null, 0f))
    internal var playing by mutableStateOf(false)
    internal var playbackDuration by mutableLongStateOf(0L)
    internal var loading by mutableStateOf(false)
    internal var showSelectionDialog by mutableStateOf(false)
    internal var selectionImporting by mutableStateOf(false)
    internal var selectionError by mutableStateOf<String?>(null)
    internal var audioName by mutableStateOf("Select audio")
    internal var lyricsName by mutableStateOf("Select lyrics (optional)")
    internal var translationName by mutableStateOf("Select translation (optional)")
    internal var audioSelected by mutableStateOf(false)
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main)
    private var load: Job? = null
    private var revision = 0L
    private var artworkLoad: Job? = null
    private var fileRequest = 0
    private var toggleRequested = false
    private var selectionPlayRequested = false
    private var selectionCancelRequested = false
    private var selectionPrepared = false
    private var preparedLyrics: SyncedLyrics? = null
    private var preparedLyricsName = ""
    private var selectionJob: Job? = null
    var durationMillis: Int = 0
        private set
    var seekRequestMillis: Int = -1
        private set

    fun openLyrics(content: String, name: String) {
        load?.cancel()
        val version = ++revision
        lyrics = null
        message = "Loading $name…"
        loading = true
        durationMillis = 0
        seekRequestMillis = -1
        load = scope.launch {
            try {
                val parsed = withContext(Dispatchers.Default) { AutoParser().parse(content) }
                ensureActive()
                if (revision == version) {
                    lyrics = parsed
                    if (title == "Accompanist" || artist == "Local preview") {
                        title = parsed.title.ifBlank { name.substringBeforeLast('.') }
                        artist = parsed.artists.orEmpty().joinToString(" / ") { it.name }.ifBlank { "Local preview" }
                    }
                    durationMillis = parsed.lines.maxOfOrNull { it.end } ?: 0
                    message = name
                }
            } catch (failure: Exception) {
                if (failure is CancellationException) throw failure
                if (revision == version) message = failure.message ?: "Lyrics could not be parsed"
            } finally {
                if (revision == version) loading = false
            }
        }
    }

    fun updatePosition(millis: Int) { position.intValue = millis.coerceAtLeast(0) }
    fun setTranslation(visible: Boolean) { translation = visible }
    fun setPronunciation(visible: Boolean) { pronunciation = visible }
    fun updatePlaybackStatus(isPlaying: Boolean, duration: Int) {
        playing = isPlaying
        playbackDuration = duration.coerceAtLeast(0).toLong()
    }
    fun setTrackMetadata(trackTitle: String, trackArtist: String, artwork: NSData?) {
        title = trackTitle
        artist = trackArtist
        artworkLoad?.cancel()
        background = BackgroundVisualState(null, 0f)
        if (artwork == null || artwork.length == 0uL || artwork.length > 16uL * 1024uL * 1024uL) return
        val bytes = ByteArray(artwork.length.toInt()).also { data ->
            data.usePinned { memcpy(it.addressOf(0), artwork.bytes, artwork.length) }
        }
        artworkLoad = scope.launch {
            val bitmap = withContext(Dispatchers.Default) {
                runCatching { SkiaImage.makeFromEncoded(bytes).toComposeImageBitmap() }.getOrNull()
            }
            ensureActive()
            background = BackgroundVisualState(bitmap, 0f)
        }
    }
    fun takeFileRequest(): Int = fileRequest.also { fileRequest = 0 }
    fun takePlaybackToggle(): Boolean = toggleRequested.also { toggleRequested = false }
    fun takeSelectionPlayRequest(): Boolean = selectionPlayRequested.also { selectionPlayRequested = false }
    fun takeSelectionCancelRequest(): Boolean = selectionCancelRequested.also { selectionCancelRequested = false }
    fun takePreparedSelection(): Boolean = selectionPrepared.also { selectionPrepared = false }
    fun setSelectedFile(kind: Int, name: String) {
        selectionError = null
        when (kind) {
            2 -> {
                audioSelected = true
                audioName = name
                lyricsName = "Select lyrics (optional)"
                translationName = "Select translation (optional)"
            }
            1 -> lyricsName = name
            3 -> translationName = name
        }
    }
    fun reportSelectionError(error: String) {
        selectionError = error
        selectionImporting = false
    }
    fun prepareSelectedLyrics(content: String?, name: String, translation: String?) {
        selectionJob?.cancel()
        selectionJob = scope.launch {
            try {
                val parsed = withContext(Dispatchers.Default) { parseSelectedLyrics(content, translation) }
                ensureActive()
                preparedLyrics = parsed
                preparedLyricsName = name
                // Swift starts the already validated audio on its next display frame.
                selectionPrepared = true
            } catch (failure: Exception) {
                if (failure is CancellationException) throw failure
                reportSelectionError(failure.message ?: "Lyrics could not be parsed")
            }
        }
    }
    fun completeSelection() {
        load?.cancel()
        revision++
        lyrics = preparedLyrics
        durationMillis = preparedLyrics?.lines?.maxOfOrNull { it.end } ?: 0
        seekRequestMillis = -1
        position.intValue = 0
        message = if (preparedLyrics == null) "No timed lyrics selected." else preparedLyricsName
        loading = false
        selectionImporting = false
        showSelectionDialog = false
    }
    internal fun openSongSelection() {
        selectionError = null
        showSelectionDialog = true
    }
    internal fun dismissSongSelection() {
        selectionJob?.cancel()
        fileRequest = 0
        selectionPlayRequested = false
        selectionPrepared = false
        preparedLyrics = null
        selectionCancelRequested = true
        selectionImporting = false
        showSelectionDialog = false
    }
    internal fun requestSelectionPlay() {
        if (!audioSelected || selectionImporting) return
        selectionError = null
        selectionImporting = true
        selectionPlayRequested = true
    }
    internal fun requestFile(kind: Int) { fileRequest = kind }
    internal fun requestPlaybackToggle() { toggleRequested = true }
    fun takeSeekRequest(): Int = seekRequestMillis.also { seekRequestMillis = -1 }
    internal fun requestSeek(millis: Int) { seekRequestMillis = millis }
    fun close() { scope.cancel() }
}

fun MainViewController(controller: IosLyricsController): UIViewController = ComposeUIViewController {
    AccompanistTheme(darkTheme = true) {
        PlayerSurface(controller.background) {
            val state = rememberLyricsLazyListState()
            PlayerLayout(
                title = controller.title,
                artist = controller.artist,
                metadataTimeMillis = { controller.position.intValue },
                hasArtwork = controller.background.bitmap != null,
                artwork = { modifier ->
                    PlayerCover(controller.background.bitmap, modifier)
                },
                modifier = Modifier.statusBarsPadding().navigationBarsPadding(),
                controls = {
                    PlayerControls(controller::openSongSelection, controller.translation, controller.pronunciation,
                        { controller.setTranslation(!controller.translation) },
                        { controller.setPronunciation(!controller.pronunciation) })
                },
                lyrics = { modifier, anchor, bottomFade ->
                    key(controller.lyrics) {
                        PlayerLyricsPanel(state, controller.lyrics, { controller.position.intValue },
                            controller.translation, controller.pronunciation,
                            onLineClicked = { controller.requestSeek(it.start) },
                            modifier = modifier, anchor = anchor, bottomFade = bottomFade,
                            loading = controller.loading, emptyMessage = controller.message)
                    }
                },
            )
            if (controller.showSelectionDialog) {
                SongSelectionDialogContent(
                    audioName = controller.audioName, lyricsName = controller.lyricsName,
                    translationName = controller.translationName,
                    audioSelected = controller.audioSelected,
                    isImporting = controller.selectionImporting, isReady = true,
                    error = controller.selectionError,
                    onSelectAudio = { controller.requestFile(2) },
                    onSelectLyrics = { controller.requestFile(1) },
                    onSelectTranslation = { controller.requestFile(3) },
                    onPlay = controller::requestSelectionPlay,
                    onDismissRequest = controller::dismissSongSelection,
                )
            }
        }
    }
}
