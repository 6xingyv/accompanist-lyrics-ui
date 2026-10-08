package com.mocharealm.accompanist.sample

import com.mocharealm.accompanist.lyrics.core.model.synced.SyncedLine
import com.mocharealm.accompanist.sample.ui.composable.player.parseSelectedLyrics
import kotlin.test.*

class SelectedLyricsTest {
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
