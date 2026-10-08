package com.mocharealm.accompanist.sample.desktop

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import kotlinx.coroutines.runInterruptible
import kotlinx.coroutines.withTimeout
import org.freedesktop.dbus.DBusPath
import org.freedesktop.dbus.annotations.DBusInterfaceName
import org.freedesktop.dbus.connections.impl.DBusConnection
import org.freedesktop.dbus.connections.impl.DBusConnectionBuilder
import org.freedesktop.dbus.interfaces.DBus
import org.freedesktop.dbus.interfaces.DBusInterface
import org.freedesktop.dbus.interfaces.Properties
import org.freedesktop.dbus.types.Variant

@DBusInterfaceName("org.mpris.MediaPlayer2.Player")
internal interface MprisPlayer : DBusInterface {
    fun SetPosition(trackId: DBusPath, position: Long)
    fun PlayPause()
}

internal class MprisMediaSession : MediaSession {
    private var connection: DBusConnection? = null
    private val lock = Mutex()
    private var activeSource: String? = null
    private fun bus(): DBusConnection = connection ?: DBusConnectionBuilder.forSessionBus().withShared(false)
        .build().also { connection = it }
    private val path = "/org/mpris/MediaPlayer2"
    private val playerInterface = "org.mpris.MediaPlayer2.Player"

    private fun unwrap(value: Any?): Any? = if (value is Variant<*>) value.value else value
    private fun read(source: String): MediaSnapshot {
        val properties = bus().getRemoteObject(source, path, Properties::class.java)
        val values = properties.GetAll(playerInterface)
        val metadata = unwrap(values["Metadata"]) as? Map<*, *> ?: emptyMap<Any, Any>()
        fun value(key: String) = unwrap(values[key])
        fun meta(key: String) = unwrap(metadata[key])
        val artist = when (val names = meta("xesam:artist")) {
            is List<*> -> names.joinToString(", ")
            is Array<*> -> names.joinToString(", ")
            else -> names?.toString().orEmpty()
        }
        return MediaSnapshot(source, meta("mpris:trackid")?.toString().orEmpty(),
            meta("xesam:title")?.toString().orEmpty(), artist,
            (value("Position") as? Number)?.toLong()?.div(1000) ?: 0L,
            (meta("mpris:length") as? Number)?.toLong()?.div(1000) ?: 0L,
            value("PlaybackStatus") == "Playing", value("CanSeek") == true,
            (value("Rate") as? Number)?.toFloat() ?: 1f,
            artwork = meta("mpris:artUrl")?.toString())
    }

    override suspend fun snapshot(): MediaSnapshot? = lock.withLock {
        withTimeout(3500) { runInterruptible(Dispatchers.IO) {
            try {
                val names = bus().getRemoteObject("org.freedesktop.DBus", "/org/freedesktop/DBus",
                    DBus::class.java).ListNames().filter { it.startsWith("org.mpris.MediaPlayer2.") }
                val candidates = names.mapNotNull { name -> runCatching { read(name) }.getOrNull() }
                (candidates.firstOrNull { it.source == activeSource && it.playing }
                    ?: candidates.firstOrNull { it.playing }
                    ?: candidates.firstOrNull { it.source == activeSource }
                    ?: candidates.firstOrNull()).also { activeSource = it?.source }
            } catch (failure: Exception) {
                connection?.close()
                connection = null
                throw failure
            }
        }
        }
    }

    override suspend fun seek(snapshot: MediaSnapshot, position: Long) = lock.withLock {
        withContext(Dispatchers.IO) {
            val current = read(snapshot.source)
            if (current.key != snapshot.key || !current.canSeek || current.trackId.isEmpty()) false
            else {
                bus().getRemoteObject(current.source, path, MprisPlayer::class.java)
                    .SetPosition(DBusPath(current.trackId), position.coerceAtLeast(0) * 1000L)
                true
            }
        }
    }
    override suspend fun togglePlayback(snapshot: MediaSnapshot) = lock.withLock {
        withContext(Dispatchers.IO) {
            if (read(snapshot.source).key != snapshot.key) false else {
                bus().getRemoteObject(snapshot.source, path, MprisPlayer::class.java).PlayPause()
                true
            }
        }
    }
    override fun close() { connection?.close(); connection = null }
}
