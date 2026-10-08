package com.mocharealm.accompanist.lyrics.ui.internal.preparation

import com.mocharealm.accompanist.lyrics.core.model.SyncedLyrics
import com.mocharealm.accompanist.lyrics.core.model.karaoke.KaraokeLine
import com.mocharealm.accompanist.lyrics.core.model.karaoke.PhoneticLevel
import com.mocharealm.accompanist.lyrics.core.parser.TTMLParser
import com.mocharealm.accompanist.lyrics.core.utils.PhoneticProvider
import com.mocharealm.accompanist.lyrics.ui.internal.test.prepare
import com.mocharealm.accompanist.lyrics.ui.internal.playback.LyricsPlaybackTimeline
import com.mocharealm.accompanist.lyrics.ui.preparation.PreparedLyrics
import java.nio.file.Files
import java.nio.file.Path
import kotlin.test.*

class TTMLPhoneticPreparationTest {
    private val parser = TTMLParser(object : PhoneticProvider {
        override val phoneticLevel = PhoneticLevel.SYLLABLE
        override fun getPhonetic(string: String): String = error("Existing TTML caption must be preserved")
    })

    @Test
    fun suppliedPronunciationSharesOriginalRowsAndPlaybackDespiteMetadataTimeDifferences() {
        val fixture = """<tt xmlns="http://www.w3.org/ns/ttml" xmlns:itunes="http://music.apple.com/lyric-ttml-internal" xml:lang="ja"><head><metadata><transliterations><transliteration>
            <text for="L1"><span begin="1" end="2.011">ki </span><span begin="2.011" end="2.5">mo</span><span begin="2.5" end="3">chi</span></text>
            </transliteration></transliterations></metadata></head><body><div>
            <p begin="1" end="3" itunes:key="L1"><span begin="1" end="2">気</span><span begin="2" end="3">持ち</span></p>
            </div></body></tt>"""
        val line = parser.parse(fixture).lines.single() as KaraokeLine
        val prepared = prepare(line)
        assertNull(prepared.phonetic)
        val captions = prepared.rows.flatMap { it.runs }.flatMap { it.groups }.flatMap { it.units }.mapNotNull { it.phonetic }
        assertEquals("ki mochi", captions.joinToString(" ") { it.layoutInput.text.text })
        assertEquals(listOf(1000 to 2000, 2000 to 3000), prepared.source.syllables.map { it.start to it.end })
        val scene = PreparedLyrics(listOf(prepared))
        assertEquals(listOf(prepared), scene.allLines, "No independent caption line")
        val timeline = LyricsPlaybackTimeline(SyncedLyrics(listOf(line)), scene)
        timeline.update(2005)
        assertEquals(2005, timeline.state.row(prepared.rows.single()).time.intValue)
        assertEquals(listOf(0), timeline.focus.value.allIndices)
    }

    @Test
    fun optionalLocalTtmlLosesNoPronunciationDuringPreparation() {
        val path = System.getenv("LYRICS_VALIDATION_TTML") ?: return
        val lines = parser.parse(Files.readString(Path.of(path))).lines.map { it as KaraokeLine }
        var count = 0
        lines.forEach { line ->
            val prepared = prepare(line)
            assertNull(line.phonetic, "Supplied fragments must attach to original words")
            assertNull(prepared.phonetic)
            assertEquals(line.syllables.map { it.start to it.end }, prepared.source.syllables.map { it.start to it.end })
            val supplied = line.syllables.mapNotNull { it.phonetic }.joinToString("")
            val rendered = prepared.rows.flatMap { it.runs }.flatMap { it.groups }.flatMap { it.units }
                .mapNotNull { it.phonetic?.layoutInput?.text?.text }.joinToString("")
            fun letters(text: String) = text.filterNot { it.isWhitespace() }
            assertEquals(letters(supplied), letters(rendered), "Attached caption at ${line.start}")
            assertEquals(1, PreparedLyrics(listOf(prepared)).allLines.size)
            count++
        }
        println("Local TTML layouts: attached-captions=$count; all pronunciation uses original rows and timing")
    }
}
