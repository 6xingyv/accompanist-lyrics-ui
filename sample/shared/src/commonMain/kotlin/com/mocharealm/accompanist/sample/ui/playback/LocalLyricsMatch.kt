package com.mocharealm.accompanist.sample.ui.playback

/** Callers supply names from the audio file's own directory only. */
fun matchingLyricsName(audioName: String, names: List<String>): String? {
    val base = audioName.substringBeforeLast('.', audioName)
    return listOf("ttml", "lrc", "elrc", "lys", "krc").firstNotNullOfOrNull { extension ->
        names.firstOrNull { it.equals("$base.$extension", ignoreCase = true) }
    }
}
