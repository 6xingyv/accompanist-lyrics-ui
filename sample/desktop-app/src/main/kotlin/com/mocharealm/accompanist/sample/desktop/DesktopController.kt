package com.mocharealm.accompanist.sample.desktop

import androidx.compose.ui.graphics.toComposeImageBitmap
import com.mocharealm.accompanist.lyrics.core.model.SyncedLyrics
import com.mocharealm.accompanist.sample.ui.playback.PlaybackSnapshot
import com.mocharealm.accompanist.sample.ui.composable.background.BackgroundVisualState
import kotlinx.coroutines.*
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.*
import java.io.ByteArrayInputStream
import java.net.URI
import java.net.http.HttpClient
import java.net.http.HttpRequest
import java.net.http.HttpResponse
import java.nio.file.Path
import java.time.Duration
import java.util.Base64
import java.util.concurrent.atomic.AtomicLong
import java.util.prefs.Preferences
import javax.imageio.ImageIO
import kotlin.math.abs

internal data class DesktopUiState(
    val lyrics: SyncedLyrics? = null,
    val title: String = "Accompanist",
    val artist: String = "",
    val source: String = "",
    val fileName: String? = null,
    val following: Boolean = true,
    val playing: Boolean = false,
    val duration: Long = 0,
    val canSeek: Boolean = false,
    val loading: Boolean = false,
    val message: String? = "Waiting for a media player…",
    val actionError: String? = null,
    val background: BackgroundVisualState = BackgroundVisualState(null, 0f),
    val showTranslation: Boolean = true,
    val showPhonetic: Boolean = true,
)

internal class DesktopController(
    initialConfig: DesktopConfig,
    private val configLoader: () -> DesktopConfig = {
        DesktopConfig.load(arrayOf("--config", initialConfig.configFile.toString()))
    },
    private val sessionFactory: () -> MediaSession = ::createMediaSession,
) : AutoCloseable {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private var session = sessionFactory()
    private val mutableConfiguration = MutableStateFlow(initialConfig)
    val configuration = mutableConfiguration.asStateFlow()
    val config: DesktopConfig get() = configuration.value
    private var directory = LyricsDirectory(config.lyricsDir, config.recursive)
    private val preferences = Preferences.userRoot().node("com/mocharealm/accompanist/sample")
    private val mutableState = MutableStateFlow(DesktopUiState(
        showTranslation = preferences.getBoolean("showTranslation", true),
        showPhonetic = preferences.getBoolean("showPhonetic", true),
    ))
    val state = mutableState.asStateFlow()
    private val mutablePlayback = MutableStateFlow(PlaybackSnapshot())
    val playback = mutablePlayback.asStateFlow()
    @Volatile private var media: MediaSnapshot? = null
    @Volatile private var following = true
    private val sequence = AtomicLong()
    private val stateLock = Any()
    private var modeRevision = 0L
    private var lyricsJob: Job? = null
    private var artworkJob: Job? = null
    private var artworkKey: String? = null
    private var loadedTrack: String? = null
    private val appleClock = AppleMusicClock()
    @Volatile private var pending: PendingSeek? = null
    private val commands = Channel<Command>(Channel.UNLIMITED)
    private data class PendingSeek(val id: Long, val mediaKey: String, val position: Long,
        val issued: Long, val accepted: Boolean = false)
    private sealed interface Command {
        val modeRevision: Long
        data class Seek(val snapshot: MediaSnapshot, val position: Long, val id: Long,
            override val modeRevision: Long) : Command
        data class Toggle(val snapshot: MediaSnapshot, override val modeRevision: Long) : Command
        data object RescanMedia : Command { override val modeRevision = -1L }
        data object ReloadConfiguration : Command { override val modeRevision = -1L }
    }

    init {
        scope.launch {
            try {
                while (isActive) {
                    var command = commands.tryReceive().getOrNull()
                    while (command != null) {
                        handle(command)
                        command = commands.tryReceive().getOrNull()
                    }
                    try {
                        val sample = session.snapshot()
                        synchronized(stateLock) {
                            media = sample
                            if (following) {
                                if (sample != null) accept(sample) else pauseMissingSession()
                            }
                        }
                    } catch (failure: Exception) {
                        if (failure is CancellationException && failure !is TimeoutCancellationException) throw failure
                        synchronized(stateLock) {
                            if (following) {
                                mutableState.update { it.copy(playing = false, canSeek = false,
                                    message = failure.message ?: "Media session unavailable") }
                                pauseClock()
                            }
                        }
                    }
                    synchronized(stateLock) {
                        if (!following) {
                            val preview = mutablePlayback.value
                            if (preview.isPlaying && preview.duration > 0 &&
                                preview.positionAt(monotonicMillis()) >= preview.duration) {
                                pauseClock()
                                mutableState.update { it.copy(playing = false) }
                            }
                        }
                    }
                    withTimeoutOrNull(500) { commands.receive() }?.let { handle(it) }
                }
            } finally { session.close() }
        }
    }

    private suspend fun handle(command: Command) {
        if (command == Command.RescanMedia || command == Command.ReloadConfiguration) {
            try {
                if (command == Command.RescanMedia) {
                    // Reconnect on the polling coroutine, after any pending native operation.
                    val replacement = sessionFactory()
                    session.close()
                    session = replacement
                    synchronized(stateLock) {
                        media = null
                        followMedia()
                    }
                } else {
                    val replacement = configLoader()
                    val replacementDirectory = LyricsDirectory(replacement.lyricsDir, replacement.recursive)
                    replacementDirectory.rescan()
                    synchronized(stateLock) {
                        mutableConfiguration.value = replacement
                        directory = replacementDirectory
                        if (following) {
                            modeRevision++
                            lyricsJob?.cancel()
                            artworkJob?.cancel()
                            pending = null
                            artworkKey = null
                            loadedTrack = null
                            media?.let(::accept)
                        }
                    }
                }
            } catch (failure: Exception) {
                if (failure is CancellationException) throw failure
                reportError(failure.message ?: "Unable to refresh desktop settings")
            }
            return
        }
        if (synchronized(stateLock) { !following || modeRevision != command.modeRevision }) return
        try {
            when (command) {
                is Command.Seek -> {
                    if (pending?.id != command.id || !following) return
                    val accepted = session.seek(command.snapshot, command.position)
                    synchronized(stateLock) {
                        if (pending?.id == command.id) {
                            pending = pending?.copy(accepted = accepted)
                            if (!accepted) {
                                pending = null
                                mutableState.update { it.copy(message = "The player rejected seeking") }
                            }
                        }
                    }
                }
                is Command.Toggle -> if (following) session.togglePlayback(command.snapshot)
                Command.RescanMedia, Command.ReloadConfiguration -> Unit
            }
        } catch (failure: Exception) {
            if (failure is CancellationException) throw failure
            synchronized(stateLock) {
                if (command is Command.Seek && pending?.id == command.id) pending = null
                if (following) mutableState.update {
                    it.copy(message = failure.message ?: "Playback command failed")
                }
            }
        }
    }

    private fun accept(sample: MediaSnapshot) {
        val trackChanged = loadedTrack != sample.key
        if (trackChanged) {
            loadedTrack = sample.key
            pending = null
            appleClock.reset(sample)
            loadForTrack(sample)
        }
        val waiting = pending
        val now = monotonicMillis()
        val expected = mutablePlayback.value.positionAt(now).toLong()
        val samplePosition = sample.position + if (sample.playing)
            ((now - sample.sampledAt).coerceAtLeast(0) * sample.speed).toLong() else 0L
        val timeout = if (waiting?.accepted == true) 15_000 else 5_000
        val ignoreOldPosition = waiting != null && waiting.mediaKey == sample.key &&
            now - waiting.issued < timeout && abs(samplePosition - expected) > 1000
        if (!ignoreOldPosition) {
            if (pending?.id == waiting?.id) pending = null
            val isWindowsApple = desktopPlatform() == DesktopPlatform.Windows &&
                sample.source.contains("AppleMusic", ignoreCase = true)
            if (waiting != null && isWindowsApple) appleClock.reset(sample)
            val position = if (isWindowsApple) appleClock.accept(sample) else sample.position
            val previous = mutablePlayback.value
            val projectedPosition = position + if (sample.playing)
                ((now - sample.sampledAt).coerceAtLeast(0) * sample.speed).toLong() else 0L
            val changedPosition = trackChanged || abs(projectedPosition - previous.positionAt(now)) > 1000
            mutablePlayback.value = PlaybackSnapshot(sample.playing, position, sample.duration,
                sample.sampledAt, sample.speed, if (changedPosition) sequence.incrementAndGet()
                else previous.discontinuity)
        } else mutablePlayback.update { it.copy(position = it.positionAt(now).toLong(),
            sampledAtMillis = now, isPlaying = sample.playing, speed = sample.speed) }
        mutableState.update { it.copy(title = sample.title, artist = sample.artist,
            source = sample.source, playing = sample.playing, duration = sample.duration,
            canSeek = sample.canSeek) }
        val newArtworkKey = "${sample.key}\u0000${sample.artwork.orEmpty()}"
        if (artworkKey != newArtworkKey) {
            artworkKey = newArtworkKey
            val revision = modeRevision
            artworkJob?.cancel()
            artworkJob = scope.launch(Dispatchers.Default) {
                val background = sample.artwork?.let {
                    runCatching { loadArtwork(it) }.getOrNull()
                } ?: BackgroundVisualState(null, 0f)
                ensureActive()
                synchronized(stateLock) {
                    if (following && modeRevision == revision && artworkKey == newArtworkKey)
                        mutableState.update { it.copy(background = background) }
                }
            }
        }
    }

    private fun loadForTrack(sample: MediaSnapshot) {
        val revision = modeRevision
        val matchingDirectory = directory
        val matchingConfig = config
        lyricsJob?.cancel()
        mutableState.update { it.copy(lyrics = null, fileName = null, loading = true, message = null) }
        lyricsJob = scope.launch {
            try {
                val started = System.nanoTime()
                val file = matchingDirectory.match(sample.title, sample.artist)
                val lyrics = file?.let(::parseDesktopLyrics)
                if (matchingConfig.frameTiming) System.err.println("[lyrics] match+parse=" +
                    "${(System.nanoTime() - started) / 1_000_000.0}ms file=$file")
                ensureActive()
                synchronized(stateLock) {
                    if (following && modeRevision == revision && loadedTrack == sample.key) {
                        mutableState.update { it.copy(lyrics = lyrics,
                            fileName = file?.fileName?.toString(), loading = false,
                            message = if (lyrics == null) "No matching lyrics in ${matchingConfig.lyricsDir}" else null) }
                    }
                }
            } catch (failure: Exception) {
                if (failure is CancellationException) throw failure
                synchronized(stateLock) {
                    if (following && modeRevision == revision && loadedTrack == sample.key) {
                        mutableState.update { it.copy(loading = false, message = failure.message) }
                    }
                }
            }
        }
    }

    fun followMedia(): Unit = synchronized(stateLock) {
        modeRevision++
        lyricsJob?.cancel()
        artworkJob?.cancel()
        following = true
        loadedTrack = null
        artworkKey = null
        pending = null
        pauseClock()
        mutableState.update { it.copy(following = true, lyrics = null, fileName = null,
            title = "Accompanist", artist = "", source = "", duration = 0,
            playing = false, canSeek = false, loading = false,
            message = "Waiting for a media player…") }
        media?.let(::accept)
    }

    fun openPreview(file: Path): Unit = synchronized(stateLock) {
        val revision = ++modeRevision
        following = false
        pending = null
        lyricsJob?.cancel()
        artworkJob?.cancel()
        artworkKey = null
        mutableState.update { it.copy(following = false, loading = true, playing = false,
            lyrics = null, fileName = null, canSeek = false, duration = 0,
            title = file.fileName.toString(), artist = "Local timing preview", source = "",
            message = null, background = BackgroundVisualState(null, 0f)) }
        mutablePlayback.value = PlaybackSnapshot(sampledAtMillis = monotonicMillis(),
            discontinuity = sequence.incrementAndGet())
        lyricsJob = scope.launch {
            try {
                val parsed = parseDesktopLyrics(file)
                ensureActive()
                val duration = parsed.lines.maxOf { it.end }.toLong().coerceAtLeast(1)
                synchronized(stateLock) {
                    if (!following && modeRevision == revision) {
                        preferences.put("lastLyrics", file.toAbsolutePath().toString())
                        mutablePlayback.value = PlaybackSnapshot(duration = duration,
                            sampledAtMillis = monotonicMillis(), discontinuity = sequence.incrementAndGet())
                        mutableState.update { it.copy(lyrics = parsed, fileName = file.fileName.toString(),
                            duration = duration, canSeek = true, loading = false, message = null) }
                    }
                }
            } catch (failure: Exception) {
                if (failure is CancellationException) throw failure
                synchronized(stateLock) {
                    if (!following && modeRevision == revision)
                        mutableState.update { it.copy(loading = false, message = failure.message) }
                }
            }
        }
    }

    fun seek(position: Long): Unit = synchronized(stateLock) {
        val sample = media
        if (!mutableState.value.canSeek) return@synchronized
        if (following && (sample == null || !sample.canSeek)) return@synchronized
        val now = monotonicMillis()
        val bounded = position.coerceIn(0, mutableState.value.duration.takeIf { it > 0 } ?: Int.MAX_VALUE.toLong())
        val id = sequence.incrementAndGet()
        if (following && sample != null) {
            pending = PendingSeek(id, sample.key, bounded, now)
            appleClock.reset(sample.copy(position = bounded, sampledAt = now))
            commands.trySend(Command.Seek(sample, bounded, id, modeRevision))
        }
        mutablePlayback.update { it.copy(position = bounded, sampledAtMillis = now, discontinuity = id) }
    }
    fun togglePlayback() = synchronized(stateLock) {
        if (following) media?.let { commands.trySend(Command.Toggle(it, modeRevision)) } else {
            val now = monotonicMillis()
            mutablePlayback.update { it.copy(isPlaying = !it.isPlaying,
                position = if (it.positionAt(now) >= it.duration && !it.isPlaying) 0
                    else it.positionAt(now).toLong(), sampledAtMillis = now,
                discontinuity = sequence.incrementAndGet()) }
            mutableState.update { it.copy(playing = mutablePlayback.value.isPlaying) }
        }
    }
    fun toggleTranslation() {
        mutableState.update { it.copy(showTranslation = !it.showTranslation) }
        preferences.putBoolean("showTranslation", mutableState.value.showTranslation)
    }
    fun togglePhonetic() {
        mutableState.update { it.copy(showPhonetic = !it.showPhonetic) }
        preferences.putBoolean("showPhonetic", mutableState.value.showPhonetic)
    }
    fun reloadLyrics() = synchronized(stateLock) {
        directory.invalidate()
        if (following) media?.let(::loadForTrack)
        else mutableState.value.fileName?.let { reopenLastPreview() }
        if (media == null) {
            val matchingDirectory = directory
            scope.launch {
                try { matchingDirectory.rescan() }
                catch (failure: Exception) {
                    if (failure is CancellationException) throw failure
                    reportError(failure.message ?: "Unable to scan lyrics")
                }
            }
        }
    }
    fun rescanMediaSessions() { commands.trySend(Command.RescanMedia) }
    fun reloadConfiguration() { commands.trySend(Command.ReloadConfiguration) }
    fun reopenLastPreview() {
        val path = preferences.get("lastLyrics", null)?.let(Path::of)
        if (path != null && java.nio.file.Files.isRegularFile(path)) openPreview(path)
        else reportError("The previous preview file is unavailable")
    }
    fun reportError(message: String) { mutableState.update { it.copy(message = message, actionError = message) } }
    fun dismissActionError() { mutableState.update { it.copy(actionError = null) } }
    private fun pauseClock() {
        val now = monotonicMillis()
        mutablePlayback.update { it.copy(isPlaying = false, position = it.positionAt(now).toLong(),
            sampledAtMillis = now) }
    }
    private fun pauseMissingSession() {
        pauseClock()
        loadedTrack = null
        lyricsJob?.cancel()
        mutableState.update { it.copy(playing = false, canSeek = false,
            loading = false,
            message = "Waiting for a media player…") }
    }
    override fun close() {
        scope.cancel()
        commands.close()
    }
}

private fun loadArtwork(reference: String): BackgroundVisualState? {
    val limit = 16 * 1024 * 1024
    val bytes = when {
        reference.startsWith("data:") -> Base64.getDecoder().decode(reference.substringAfter("base64,"))
        reference.startsWith("file:") -> {
            val path = Path.of(URI(reference))
            require(java.nio.file.Files.size(path) <= limit) { "Artwork exceeds 16 MB" }
            java.nio.file.Files.readAllBytes(path)
        }
        reference.startsWith("https:") || reference.startsWith("http:") -> {
            val client = HttpClient.newBuilder().connectTimeout(Duration.ofSeconds(3))
                .followRedirects(HttpClient.Redirect.NORMAL).build()
            val request = HttpRequest.newBuilder(URI(reference)).timeout(Duration.ofSeconds(5)).GET().build()
            val response = client.send(request, HttpResponse.BodyHandlers.ofInputStream())
            response.body().use { input ->
                require(response.statusCode() in 200..299) { "Artwork HTTP ${response.statusCode()}" }
                input.readNBytes(limit + 1)
            }
        }
        else -> return null
    }
    require(bytes.size <= limit) { "Artwork exceeds 16 MB" }
    return ImageIO.createImageInputStream(ByteArrayInputStream(bytes)).use { input ->
        val readers = ImageIO.getImageReaders(input)
        if (!readers.hasNext()) return@use null
        val reader = readers.next()
        try {
            reader.input = input
            val dimension = maxOf(reader.getWidth(0), reader.getHeight(0))
            val sampling = ((dimension + 1023) / 1024).coerceAtLeast(1)
            val params = reader.defaultReadParam.apply { setSourceSubsampling(sampling, sampling, 0, 0) }
            val image = reader.read(0, params)
            // Match Android's sampled Rec. 709 luminance before shared background preparation.
            val step = (minOf(image.width, image.height) / 50).coerceAtLeast(1)
            var total = 0.0
            var count = 0
            for (y in 0 until image.height step step) for (x in 0 until image.width step step) {
                val color = image.getRGB(x, y)
                total += .2126 * (color shr 16 and 255) + .7152 * (color shr 8 and 255) +
                    .0722 * (color and 255)
                count++
            }
            BackgroundVisualState(image.toComposeImageBitmap(), (total / count / 255).toFloat())
        } finally { reader.dispose() }
    }
}
