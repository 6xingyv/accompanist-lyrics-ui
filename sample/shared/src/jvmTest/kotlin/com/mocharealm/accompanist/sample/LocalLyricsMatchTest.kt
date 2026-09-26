package com.mocharealm.accompanist.sample

import com.mocharealm.accompanist.sample.ui.playback.matchingLyricsName
import kotlin.test.*

class LocalLyricsMatchTest {
    @Test
    fun sameBaseNameAndFormatPriority() {
        assertEquals(
            "Track.Live.TTML",
            matchingLyricsName("Track.Live.flac", listOf("Track.Live.lrc", "Track.Live.TTML")),
        )
        assertEquals("song.LRC", matchingLyricsName("Song.mp3", listOf("song.LRC")))
    }

    @Test
    fun doesNotSelectTranslationOrUnrelatedNames() {
        assertNull(
            matchingLyricsName("Song.mp3", listOf("Song-translation.lrc", "Song2.ttml", "Song.txt"))
        )
        assertNull(matchingLyricsName("Song.mp3", emptyList()))
    }
}
