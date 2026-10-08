@file:OptIn(kotlinx.cinterop.ExperimentalForeignApi::class)

package com.mocharealm.accompanist.lyrics.ui.internal.rendering

import platform.Foundation.NSRecursiveLock

internal actual class LyricsResourceLock actual constructor() {
    private val lock = NSRecursiveLock()
    actual fun <T> withLock(block: () -> T): T {
        lock.lock()
        return try { block() } finally { lock.unlock() }
    }
}
