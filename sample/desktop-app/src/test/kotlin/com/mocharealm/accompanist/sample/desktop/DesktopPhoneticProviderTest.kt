package com.mocharealm.accompanist.sample.desktop

import com.mocharealm.accompanist.lyrics.core.model.karaoke.KaraokeLine
import com.mocharealm.accompanist.lyrics.core.model.synced.SyncedLine
import org.junit.Rule
import org.junit.rules.TemporaryFolder
import java.nio.file.Files
import java.util.concurrent.Callable
import java.util.concurrent.Executors
import kotlin.test.*

class DesktopPhoneticProviderTest {
    @get:Rule val temporary = TemporaryFolder()

    @Test fun localLyricsGeneratePronunciationWithoutChangingTimingOrText() {
        val file = temporary.newFile("Song.lrc").toPath()
        Files.writeString(file, "\uFEFF[00:01.00]银行行走\n[00:03.00]你好\n[00:05.00]Hello")
        val lines = parseDesktopLyrics(file).lines.map { it as SyncedLine }
        assertEquals(listOf("yin hang xing zou", "ni hao", null), lines.map { it.phonetic })
        assertEquals(listOf("银行行走", "你好", "Hello"), lines.map { it.content })
        assertEquals(listOf(1000, 3000, 5000), lines.map { it.start })
    }

    @Test fun packagedDictionariesAndKoreanRomanizationAreAvailable() {
        for ((text, language, expected) in listOf(
            Triple("银行行走", "zh-CN", "yin hang xing zou"),
            Triple("垃圾", "zh-TW", "le se"),
            Triple("銀行", "yue-HK", "ngan hong"),
            Triple("日本語", "ja", "nihongo"),
            Triple("한국", "ko", "hangug"),
        )) assertEquals(expected, DesktopPhoneticProvider.getPhonetic(text, language), language)
    }

    @Test fun suppliedTtmlCaptionsKeepPriorityAndSyllableTimes() {
        val file = temporary.newFile("Song.ttml").toPath()
        Files.writeString(file, """<tt xmlns="http://www.w3.org/ns/ttml"
            xmlns:itunes="http://music.apple.com/lyric-ttml-internal" xml:lang="ja">
            <head><metadata><transliterations><transliteration xml:lang="ja-Latn">
            <text for="L1"><span begin="1" end="2.011">ki </span>
            <span begin="2.011" end="2.5">mo</span><span begin="2.5" end="3">chi</span></text>
            </transliteration></transliterations></metadata></head>
            <body><div><p begin="1" end="3" itunes:key="L1"><span begin="1" end="2">気</span>
            <span begin="2" end="3">持ち</span></p></div></body></tt>""")
        val line = parseDesktopLyrics(file).lines.single() as KaraokeLine
        assertEquals(listOf("ki", "mochi"), line.syllables.map { it.phonetic })
        assertEquals(listOf(1000 to 2000, 2000 to 3000), line.syllables.map { it.start to it.end })
        assertNull(line.phonetic)
    }

    @Test fun concurrentLoadsSharePacksWithoutSharingMutableParseState() {
        val pool = Executors.newFixedThreadPool(4)
        try {
            val readings = pool.invokeAll(List(16) { index -> Callable {
                val japanese = index % 2 == 0
                DesktopPhoneticProvider.getPhonetic(if (japanese) "日本語" else "银行行走",
                    if (japanese) "ja" else "zh-CN")
            } }).map { it.get() }
            assertEquals(List(16) { if (it % 2 == 0) "nihongo" else "yin hang xing zou" }, readings)
        } finally { pool.shutdownNow() }
    }
}
