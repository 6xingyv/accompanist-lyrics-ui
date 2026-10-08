package com.mocharealm.accompanist.lyrics.ui.internal.diagnostics

internal actual fun lyricsTraceEnabled(): Boolean = false
internal actual fun beginLyricsTrace(name: String) {}
internal actual fun endLyricsTrace() {}
internal actual fun setLyricsTraceCounter(name: String, value: Long) {}
