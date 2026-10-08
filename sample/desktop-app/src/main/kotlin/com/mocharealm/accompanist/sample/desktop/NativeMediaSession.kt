package com.mocharealm.accompanist.sample.desktop

import kotlinx.coroutines.asCoroutineDispatcher
import kotlinx.coroutines.withContext
import java.nio.file.Files
import java.nio.file.Path
import java.security.MessageDigest
import java.util.concurrent.Executors

/** JNI constructor signature is shared with native/media_jni.h. */
internal data class NativeMediaSnapshot(
    val source: String,
    val trackId: String,
    val title: String,
    val artist: String,
    val position: Long,
    val duration: Long,
    val playing: Boolean,
    val canSeek: Boolean,
    val speed: Float,
    val timelineToken: Long,
    val artwork: ByteArray?,
    val artworkUrl: String?,
    val sampledAtNanos: Long,
)

internal object NativeMediaBridge {
    init {
        val os = if (desktopPlatform() == DesktopPlatform.Windows) "windows" else "macos"
        val arch = when (System.getProperty("os.arch").lowercase()) {
            "amd64", "x86_64" -> "x64"
            "aarch64", "arm64" -> "arm64"
            else -> error("Unsupported native media architecture: ${System.getProperty("os.arch")}")
        }
        val name = System.mapLibraryName("accompanist_media")
        val bytes = checkNotNull(javaClass.getResourceAsStream("/native/$os-$arch/$name")) {
            "Native media library is missing for $os-$arch; build the desktop app on this platform"
        }.use { it.readBytes() }
        val hash = MessageDigest.getInstance("SHA-256").digest(bytes).joinToString("") { "%02x".format(it) }
        // Windows keeps a loaded DLL locked. Reuse a content-addressed cache across launches.
        val directory = Path.of(System.getProperty("java.io.tmpdir"), "accompanist-native", hash)
        Files.createDirectories(directory)
        val library = directory.resolve(name)
        if (!Files.exists(library)) {
            val temporary = Files.createTempFile(directory, "library-", ".tmp")
            try {
                Files.write(temporary, bytes)
                try { Files.move(temporary, library) }
                catch (_: java.nio.file.FileAlreadyExistsException) { /* Another instance extracted it. */ }
            } finally { Files.deleteIfExists(temporary) }
        }
        System.load(library.toAbsolutePath().toString())
    }

    external fun create(): Long
    external fun read(handle: Long): NativeMediaSnapshot?
    external fun seek(handle: Long, source: String, trackId: String, position: Long): Boolean
    external fun toggle(handle: Long, source: String, trackId: String): Boolean
    external fun release(handle: Long)
}

/** One native worker owns COM initialization and serializes all platform calls. */
internal class NativeMediaSession : MediaSession {
    private val executor = Executors.newSingleThreadExecutor {
        Thread(it, "Accompanist native media").apply { isDaemon = true }
    }
    private val dispatcher = executor.asCoroutineDispatcher()
    @Volatile private var closed = false
    private var handle = 0L // Accessed only on the native worker.
    private var artworkFile: Path? = null
    private var artworkKey: String? = null

    private fun nativeHandle(): Long {
        check(!closed) { "Media session is closed" }
        if (handle == 0L) {
            try { handle = NativeMediaBridge.create() }
            catch (failure: LinkageError) {
                throw IllegalStateException("Unable to load the native media adapter: ${failure.message}", failure)
            }
        }
        check(handle != 0L) { "Native media session initialization failed" }
        return handle
    }

    override suspend fun snapshot(): MediaSnapshot? = withContext(dispatcher) {
        val client = nativeHandle()
        val value = NativeMediaBridge.read(client) ?: return@withContext null
        val key = "${value.source}\u0000${value.trackId}"
        if (artworkKey != key) {
            artworkFile?.let { Files.deleteIfExists(it) }
            artworkFile = null
            artworkKey = key
        }
        value.artwork?.let { bytes ->
            if (artworkFile == null && bytes.isNotEmpty()) {
                require(bytes.size <= 16 * 1024 * 1024) { "Artwork exceeds 16 MB" }
                val file = Files.createTempFile("accompanist-artwork-", ".image")
                try { Files.write(file, bytes); artworkFile = file }
                catch (failure: Exception) { Files.deleteIfExists(file); throw failure }
            }
        }
        MediaSnapshot(value.source, value.trackId, value.title, value.artist, value.position,
            value.duration, value.playing, value.canSeek, value.speed, value.timelineToken,
            value.artworkUrl ?: artworkFile?.toUri()?.toString(), value.sampledAtNanos / 1_000_000L)
    }

    override suspend fun seek(snapshot: MediaSnapshot, position: Long): Boolean = withContext(dispatcher) {
        val client = nativeHandle()
        NativeMediaBridge.seek(client, snapshot.source, snapshot.trackId, position.coerceAtLeast(0))
    }

    override suspend fun togglePlayback(snapshot: MediaSnapshot): Boolean = withContext(dispatcher) {
        val client = nativeHandle()
        NativeMediaBridge.toggle(client, snapshot.source, snapshot.trackId)
    }

    @Synchronized override fun close() {
        if (closed) return
        closed = true
        // Release COM on its owning worker after any in-flight native call finishes.
        executor.execute {
            try {
                if (handle != 0L) NativeMediaBridge.release(handle)
                handle = 0L
                artworkFile?.let { Files.deleteIfExists(it) }
            } finally { dispatcher.close() }
        }
    }
}
