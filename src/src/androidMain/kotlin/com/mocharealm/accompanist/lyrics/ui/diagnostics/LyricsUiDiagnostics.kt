package com.mocharealm.accompanist.lyrics.ui.diagnostics

/** Bounded process-local diagnostics that can be exported without ADB/logcat. */
object LyricsUiDiagnostics {
    private const val MAX_LINES = 1_024
    private val lines = ArrayDeque<String>(MAX_LINES)

    init {
        System.loadLibrary("text_engine")
    }

    @Synchronized
    fun record(component: String, message: String) {
        if (lines.size == MAX_LINES) lines.removeFirst()
        lines.addLast("${System.currentTimeMillis()} [Kotlin] [$component] $message")
    }

    fun snapshot(): String {
        val kotlinLines = synchronized(this) { lines.joinToString("\n") }
        val nativeLines = runCatching { nativeSnapshot() }
            .getOrElse { "native_snapshot_error=${it.stackTraceToString()}" }
        return buildString {
            appendLine("-- lyrics-ui Kotlin events --")
            appendLine(kotlinLines)
            appendLine()
            appendLine("-- lyrics-ui Rust state and logs --")
            append(nativeLines)
        }
    }

    private external fun nativeSnapshot(): String
}
