package com.mocharealm.accompanist.sample.desktop

internal data class MediaSnapshot(
    val source: String,
    val trackId: String,
    val title: String,
    val artist: String,
    val position: Long,
    val duration: Long,
    val playing: Boolean,
    val canSeek: Boolean,
    val speed: Float = 1f,
    val timelineToken: Long = 0,
    val artwork: String? = null,
    val sampledAt: Long = monotonicMillis(),
) {
    val key get() = "$source\u0000$trackId\u0000$title\u0000$artist"
}

internal interface MediaSession : AutoCloseable {
    suspend fun snapshot(): MediaSnapshot?
    suspend fun seek(snapshot: MediaSnapshot, position: Long): Boolean
    suspend fun togglePlayback(snapshot: MediaSnapshot): Boolean
    override fun close() {}
}

internal fun createMediaSession(): MediaSession = when (desktopPlatform()) {
    DesktopPlatform.Windows, DesktopPlatform.Mac -> NativeMediaSession()
    DesktopPlatform.Linux -> MprisMediaSession()
}
