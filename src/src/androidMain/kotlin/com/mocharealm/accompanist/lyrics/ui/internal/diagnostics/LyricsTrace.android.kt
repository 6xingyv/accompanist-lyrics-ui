package com.mocharealm.accompanist.lyrics.ui.internal.diagnostics

import androidx.tracing.Trace

// androidx.tracing checks the SDK level internally and falls back on older devices.
internal actual fun lyricsTraceEnabled(): Boolean = Trace.isEnabled()

internal actual fun beginLyricsTrace(name: String) = Trace.beginSection(name)

internal actual fun endLyricsTrace() = Trace.endSection()

// Keep the Int overload: the Long overload of androidx.tracing's setCounter is API 29+ only.
internal actual fun setLyricsTraceCounter(name: String, value: Long) =
    Trace.setCounter(name, value.toInt())
