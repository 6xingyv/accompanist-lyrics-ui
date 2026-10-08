package com.mocharealm.accompanist.lyrics.ui.internal.text

/** Dictionary word boundaries, independent of lyric timing segments and the device locale. */
internal expect fun platformWordBreakBoundaries(text: String, localeTag: String?): IntArray
