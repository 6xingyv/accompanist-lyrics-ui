package com.mocharealm.accompanist.lyrics.ui.internal.rendering

/** Short cache bookkeeping sections; rasterization never holds this lock. */
internal expect class LyricsResourceLock() {
    fun <T> withLock(block: () -> T): T
}
