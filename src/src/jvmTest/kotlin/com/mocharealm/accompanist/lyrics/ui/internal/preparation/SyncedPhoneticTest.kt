package com.mocharealm.accompanist.lyrics.ui.internal.preparation

import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.unit.sp
import com.mocharealm.accompanist.lyrics.core.model.SyncedLyrics
import com.mocharealm.accompanist.lyrics.core.model.synced.SyncedLine
import com.mocharealm.accompanist.lyrics.ui.internal.test.measurer
import com.mocharealm.accompanist.lyrics.ui.internal.test.style
import com.mocharealm.accompanist.lyrics.ui.preparation.prepareLyrics
import com.mocharealm.accompanist.lyrics.ui.profile.DefaultLyricsProfiles
import kotlin.test.*

class SyncedPhoneticTest {
    @Test fun lineCaptionReachesLayoutWithoutKaraokeAnimation() {
        val source = SyncedLine("你好 한국", "translation", 100, 900, "ni hao hangug", "zh-CN")
        for (show in listOf(true, false)) {
            val prepared = prepareLyrics(SyncedLyrics(listOf(source)), DefaultLyricsProfiles, measurer,
                style, style, TextStyle(fontSize = 12.sp), 400f, 1f, show,
                translationStyle = TextStyle(fontSize = 12.sp)).lines.single()!!
            assertEquals(if (show) source.phonetic else null, prepared.phonetic?.layoutInput?.text?.text)
            assertEquals(source.start, prepared.source.start)
            assertEquals(source.end, prepared.source.end)
            assertEquals(source.languageTag, prepared.source.languageTag)
            assertTrue(prepared.rows.all { !it.animated })
        }
    }
}
