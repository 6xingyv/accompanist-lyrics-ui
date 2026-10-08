package com.mocharealm.accompanist.sample.ui.composable.player

import com.mocharealm.accompanist.lyrics.core.model.SyncedLyrics
import com.mocharealm.accompanist.lyrics.core.model.karaoke.KaraokeLine
import com.mocharealm.accompanist.lyrics.core.model.synced.SyncedLine
import com.mocharealm.accompanist.lyrics.core.parser.AutoParser

/** Manual timed translations override embedded ones at matching timestamps, as on Android. */
internal fun parseSelectedLyrics(content: String?, translation: String?): SyncedLyrics? {
    val parser = AutoParser()
    val lyrics = content?.let { parser.parse(it.removePrefix("\uFEFF")) } ?: return null
    require(lyrics.lines.isNotEmpty()) { "No supported timed lyrics in the selected file." }
    if (translation == null) return lyrics
    val translations = parser.parse(translation.removePrefix("\uFEFF")).lines.mapNotNull { line ->
        val text = when (line) {
            is KaraokeLine -> line.syllables.joinToString("") { it.content }.trim()
            is SyncedLine -> line.content.trim()
            else -> ""
        }
        text.takeIf(String::isNotBlank)?.let { line.start to it }
    }.groupBy({ it.first }, { it.second }).mapValues { it.value.first() }
    require(translations.isNotEmpty()) { "Unable to read timed translation from the selected file." }
    return lyrics.copy(lines = lyrics.lines.map { line ->
        when (line) {
            is KaraokeLine.MainKaraokeLine -> line.copy(
                translation = translations[line.start] ?: line.translation,
                accompanimentLines = line.accompanimentLines?.map {
                    it.copy(translation = translations[it.start] ?: it.translation)
                })
            is KaraokeLine.AccompanimentKaraokeLine ->
                line.copy(translation = translations[line.start] ?: line.translation)
            is SyncedLine -> line.copy(translation = translations[line.start] ?: line.translation)
            else -> line
        }
    })
}
