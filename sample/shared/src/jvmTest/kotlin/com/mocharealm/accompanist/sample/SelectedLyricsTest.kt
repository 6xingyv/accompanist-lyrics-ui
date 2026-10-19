package com.mocharealm.accompanist.sample

import com.mocharealm.accompanist.lyrics.core.model.synced.SyncedLine
import com.mocharealm.accompanist.sample.ui.composable.player.parseSelectedLyrics
import com.mocharealm.accompanist.sample.data.utils.createSamplePhonetics
import com.mocharealm.accompanist.lyrics.phonetics.ClasspathPackLoader
import com.mocharealm.accompanist.lyrics.phonetics.SharedPackLoader
import kotlin.test.*

class SelectedLyricsTest {
    @Test fun importedLyricsGeneratePhoneticsAndKeepManualTranslation() {
        val provider = createSamplePhonetics(SharedPackLoader(ClasspathPackLoader())) { it }
        val lyrics = assertNotNull(parseSelectedLyrics("[00:01.00]银行行走\n[00:03.00]你好",
            "[00:01.00]Walking past the bank", provider))
        val first = lyrics.lines.first() as SyncedLine
        assertEquals("yin hang xing zou", first.phonetic)
        assertEquals("Walking past the bank", first.translation)
        assertEquals("银行行走", first.content)
        assertEquals(1000, first.start)
        assertEquals("ni hao", (lyrics.lines[1] as SyncedLine).phonetic)
    }
    @Test fun manualTimedTranslationMatchesThePrimaryTimestamp() {
        val lyrics = assertNotNull(parseSelectedLyrics("[00:01.00]First\n[00:03.00]Second",
            "[00:01.00]译文\n[00:09.00]Unmatched"))
        assertEquals("译文", (lyrics.lines.first() as SyncedLine).translation)
        assertNull((lyrics.lines[1] as SyncedLine).translation)
    }

    @Test fun invalidSelectionDoesNotProduceEmptyParsedLyrics() {
        assertFailsWith<IllegalArgumentException> { parseSelectedLyrics("Untimed text", null) }
        assertFailsWith<IllegalArgumentException> {
            parseSelectedLyrics("[00:01.00]First", "Untimed translation")
        }
        assertNull(parseSelectedLyrics(null, null))
    }
}
