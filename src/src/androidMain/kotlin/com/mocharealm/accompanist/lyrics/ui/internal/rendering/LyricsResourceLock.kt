package com.mocharealm.accompanist.lyrics.ui.internal.rendering

internal actual class LyricsResourceLock actual constructor() {
    private val monitor = Any()
    actual fun <T> withLock(block: () -> T): T = synchronized(monitor, block)
}
