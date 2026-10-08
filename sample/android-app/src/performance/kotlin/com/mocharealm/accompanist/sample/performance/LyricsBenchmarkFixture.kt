package com.mocharealm.accompanist.sample.performance

import com.mocharealm.accompanist.lyrics.core.model.SyncedLyrics
import com.mocharealm.accompanist.lyrics.core.model.karaoke.KaraokeAlignment
import com.mocharealm.accompanist.lyrics.core.model.karaoke.KaraokeLine
import com.mocharealm.accompanist.lyrics.core.model.karaoke.KaraokeSyllable

/** Authored fixtures, independent of storage access, music playback and copyrighted songs. */
internal object LyricsBenchmarkFixture {
    private data class Phrase(val language: String, val text: List<String>, val reading: List<String>)

    private val phrases = listOf(
        Phrase("ja", listOf("朝の", "光が", "街を", "照らす"), listOf("asa no", "hikari ga", "machi o", "terasu")),
        Phrase("zh-Hans", listOf("沿着", "河流", "慢慢", "向前"), listOf("yán zhe", "hé liú", "màn màn", "xiàng qián")),
        Phrase("ko", listOf("오늘도 ", "우리는 ", "함께 ", "걸어요"), listOf("oneuldo", "urineun", "hamkke", "georeoyo")),
        // Adjacent spans form a single word; pronunciation must still follow each syllable.
        Phrase("en", listOf("Walk ", "to", "ge", "ther ", "through ", "the ", "mor", "ning"),
            listOf("wɔːk", "tə", "ɡe", "ðə", "θruː", "ðə", "mɔː", "nɪŋ")),
        Phrase("ar", listOf("نور ", "الصباح ", "فوق ", "النهر"), listOf("nur", "as sabah", "fawq", "an nahr")),
        Phrase("he", listOf("אור ", "הבוקר ", "על ", "הנהר"), listOf("or", "haboker", "al", "hanahar")),
    )

    fun create(lineCount: Int = 128): SyncedLyrics {
        var start = 1000
        val lines = List(lineCount) { index ->
            val phrase = phrases[index % phrases.size]
            val duration = 2200
            val end = start + duration
            val syllables = phrase.text.mapIndexed { unit, text ->
                KaraokeSyllable(text, start + unit * duration / phrase.text.size,
                    start + (unit + 1) * duration / phrase.text.size,
                    phrase.reading[unit], phrase.language)
            }
            val backing = if (index % 4 == 1) listOf(
                KaraokeLine.AccompanimentKaraokeLine(
                    listOf(KaraokeSyllable("Keep going", start + 500, end + 250, "kiːp ɡəʊɪŋ", "en")),
                    "继续向前", KaraokeAlignment.End, start + 500, end + 250,
                )
            ) else null
            val line = KaraokeLine.MainKaraokeLine(syllables,
                "Morning light follows us along the river, one step at a time. 清晨的光照亮向前的路。",
                if (index % 5 == 4) KaraokeAlignment.End else KaraokeAlignment.Start,
                start, end, accompanimentLines = backing, languageTag = phrase.language)
            // Include breathing dots and overlapping adjacent backing vocals.
            start = end + if (index % 8 == 7) 5500 else 100
            line
        }
        return SyncedLyrics(lines, title = "Multilingual rendering benchmark", id = "benchmark-128")
    }
}
