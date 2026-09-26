package com.mocharealm.accompanist.lyrics.ui.internal.diagnostics

internal actual fun lyricsTraceEnabled(): Boolean = android.os.Trace.isEnabled()

internal actual fun beginLyricsTrace(name: String) = android.os.Trace.beginSection(name)

internal actual fun endLyricsTrace() = android.os.Trace.endSection()
