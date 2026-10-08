package com.mocharealm.accompanist.sample.data.utils

import com.mocharealm.accompanist.lyrics.core.model.SyncedLyrics
import com.mocharealm.accompanist.lyrics.core.model.karaoke.*
import com.mocharealm.accompanist.lyrics.phonetics.*
import com.ibm.icu.text.Transliterator
import com.mocharealm.accompanist.lyrics.core.model.synced.SyncedLine
import com.mocharealm.accompanist.lyrics.core.parser.AutoParser
import com.mocharealm.accompanist.lyrics.core.utils.*
import kotlin.test.*

class AndroidPhoneticProviderTest {
    @Test fun numericReadingsUseSelectedLocaleAndKeepMainTiming() {
        for ((text, tag, expected) in listOf(Triple("2026年", "zh-CN", "er ling er liu nian"),
            Triple("２０２６年", "zh-TW", "er ling er liu nian"), Triple("2026年", "yue-HK", "ji ling ji luk nin"),
            Triple("100年", "zh-CN", "yi bai nian"), Triple("100年", "zh-TW", "yi bai nian"),
            Triple("100年", "yue-HK", "jat baak nin"),
            Triple("2人", "ja", "futari"), Triple("123", "zh-CN", "yi bai er shi san"))) {
            assertEquals(expected, provider.getPhonetic(text, tag), text)
            val source = line(text.map(Char::toString), tag)
            val converted = provider.enrich(SyncedLyrics(listOf(source))).lines.single() as KaraokeLine.MainKaraokeLine
            assertEquals(expected, converted.phonetic)
            assertEquals(source, converted.copy(phonetic = null))
        }
        assertEquals("", provider.getPhonetic("123", "es"))
    }
    @Test fun callerReadingKeepsOriginalRangesAndSuppliedCaptionPriority() {
        val forced = ProfilePhoneticProvider(PhoneticEngine(emptyList()), AsciiFormatter()) { text ->
            if (text == "星空") listOf(ReadingOverride(TextRange(0, 2),
                Pronunciation(ReadingLocale.JAPANESE, listOf(ReadingUnit("あした"))), "song-editor")) else emptyList()
        }
        val original = line(listOf("星", "空"), "ja")
        val converted = forced.enrich(SyncedLyrics(listOf(original))).lines.single() as KaraokeLine.MainKaraokeLine
        assertEquals("ashita", converted.phonetic)
        assertEquals(original, converted.copy(phonetic = null))
        val supplied = original.copy(phonetic = "file-caption")
        assertEquals(supplied, forced.enrich(SyncedLyrics(listOf(supplied))).lines.single())
    }
    @Test fun suppliedTtmlPronunciationUsesOriginalSyllableTimes() {
        val fixture = """<tt xmlns="http://www.w3.org/ns/ttml" xmlns:itunes="http://music.apple.com/lyric-ttml-internal" xml:lang="ja"><head><metadata><transliterations><transliteration xml:lang="ja-Latn"><text for="L1"><span begin="1" end="2.011">ki </span><span begin="2.011" end="2.5">mo</span><span begin="2.5" end="3">chi</span></text></transliteration></transliterations></metadata></head><body><div><p begin="1" end="3" itunes:key="L1"><span begin="1" end="2">気</span><span begin="2" end="3">持ち</span></p></div></body></tt>"""
        val lyrics = AutoParser(fallbackPhoneticProvider = provider).parse(fixture)
        val line = lyrics.lines.single() as KaraokeLine
        assertEquals(listOf("ki", "mochi"), line.syllables.map { it.phonetic })
        assertEquals(listOf(1000 to 2000, 2000 to 3000), line.syllables.map { it.start to it.end })
        assertNull(line.phonetic)
        assertEquals(lyrics, provider.enrich(lyrics))
    }

    private val provider: ProfilePhoneticProvider
    init {
        val packs = SharedPackLoader(PackLoader { name ->
            val bytes = checkNotNull(javaClass.classLoader!!.getResourceAsStream(name)) { "Missing published test pack: $name" }
                .use { it.readBytes() }
            ArrayByteSource(bytes)
        })
        val korean = Transliterator.getInstance("Hangul-Latin; Latin-ASCII")
        provider = createSamplePhonetics(packs) { korean.transliterate(it).lowercase(java.util.Locale.ROOT) }
    }
    private fun line(texts: List<String>, language: String? = "zh-CN") = KaraokeLine.MainKaraokeLine(
        syllables = texts.mapIndexed { index, text -> KaraokeSyllable(text, index * 100, (index + 1) * 100, languageTag = language) },
        translation = "translation", alignment = KaraokeAlignment.Start, start = 0, end = texts.size * 100,
    )
    @Test fun punctuationAndAsciiOutput() {
        assertEquals("", provider.getPhonetic("。？！～"))
        assertEquals("ni hao", provider.getPhonetic("你好。"))
    }
    @Test fun lineContextSurvivesTimedSyllables() {
        val source = line(listOf("银", "行", "行", "走"))
        val result = provider.enrich(SyncedLyrics(listOf(source), title = "title", id = "id"))
        val converted = result.lines.single() as KaraokeLine.MainKaraokeLine
        assertEquals(listOf("yin", "hang", "xing", "zou"), converted.syllables.map { it.phonetic })
        assertEquals(listOf("", " ", " ", " "), converted.syllables.map { it.phoneticSeparatorBefore })
        assertEquals(source.syllables.map { it.start to it.end }, converted.syllables.map { it.start to it.end })
        assertEquals("title", result.title); assertEquals("id", result.id); assertEquals(source.translation, converted.translation)
    }
    @Test fun ownedCommonPolyphonesUseWholeLineContextAndKeepTiming() {
        for ((texts, tag, expected) in listOf(
            Triple(listOf("为", "你", "着", "迷"), "zh-CN", listOf("wei", "ni", "zhao", "mi")),
            Triple(listOf("看", "着", "你"), "zh-CN", listOf("kan", "zhe", "ni")),
            Triple(listOf("朝", "夕", "湖", "泊"), "zh-TW", listOf("zhao", "xi", "hu", "bo")),
        )) {
            val original = line(texts, tag)
            val converted = provider.enrich(SyncedLyrics(listOf(original))).lines.single() as KaraokeLine.MainKaraokeLine
            assertEquals(expected, converted.syllables.map { it.phonetic })
            assertEquals(listOf("", " ", " ", " ").take(texts.size), converted.syllables.map { it.phoneticSeparatorBefore })
            assertEquals(original, converted.copy(syllables = converted.syllables.map { it.copy(phonetic = null, phoneticSeparatorBefore = "") }))
        }
        assertEquals("wei ni zhao mi", provider.getPhonetic("为你着迷"))
        val synced = SyncedLine("为你着迷", "translation", 100, 800, languageTag = "zh-CN")
        assertEquals(synced.copy(phonetic = "wei ni zhao mi"), provider.enrich(SyncedLyrics(listOf(synced))).lines.single())
    }
    @Test fun projectionBoundaryMetadataReconstructsWholeCaptionAndSupportsStandaloneRanges() {
        val request = PhoneticRequest("着迷", projectionRanges = listOf(PhoneticTextRange(0, 1), PhoneticTextRange(1, 2)))
        val response = provider.resolve(request)
        assertEquals("zhao mi", response.phonetic)
        assertEquals(listOf("", " "), response.projections.map { it.separatorBefore })
        assertEquals(response.phonetic, response.projections.joinToString("") { it.separatorBefore + it.phonetic.orEmpty() })
        assertEquals(listOf(PhoneticProjection("mi")), provider.resolve(request.copy(projectionRanges = listOf(PhoneticTextRange(1, 2)))).projections)
        val mixed = provider.resolve(PhoneticRequest("银hola行", projectionRanges = listOf(
            PhoneticTextRange(0, 1), PhoneticTextRange(1, 5), PhoneticTextRange(5, 6))))
        assertEquals(listOf("", "", " "), mixed.projections.map { it.separatorBefore })
        assertNull(mixed.projections[1].phonetic)
        assertTrue(mixed.projections.all { it.aligned })
        assertEquals(mixed.phonetic, mixed.projections.joinToString("") { it.separatorBefore + it.phonetic.orEmpty() })
    }
    @Test fun japaneseWordUsesLineFallbackWhenCharacterAlignmentIsUnknown() {
        val source = line(listOf("日", "本", "語"), "ja")
        val result = provider.enrich(SyncedLyrics(listOf(source))).lines.single() as KaraokeLine
        assertEquals("nihongo", result.phonetic)
        assertTrue(result.syllables.all { it.phonetic == null })
    }
    @Test fun japaneseContextAndCrossWordFormattingUsePublishedDataAndOriginalTiming() {
        for ((text, expected) in listOf("一人" to "hitori", "二人" to "futari", "一人前" to "ichininmae",
            "二日" to "futsuka", "二十日" to "hatsuka", "と言った" to "to itta",
            "学生の方" to "gakusei no kata", "長い間" to "nagai aida", "何の本" to "nan no hon",
            "何と何" to "nani to nani", "云った" to "itta", "咲いたーー" to "sai taaa")) {
            assertEquals(expected, provider.getPhonetic(text, "ja"), text)
        }
        val source = line(listOf("一", "人", "で", "言っ", "た"), "ja")
        val converted = provider.enrich(SyncedLyrics(listOf(source))).lines.single() as KaraokeLine.MainKaraokeLine
        assertEquals("hitori de itta", converted.phonetic)
        assertEquals(source, converted.copy(phonetic = null))
        assertEquals("gonen", provider.getPhonetic("５年", "ja"))
        val prolonged = line(listOf("咲い", "た", "ー", "ー"), "ja")
        val withCaption = provider.enrich(SyncedLyrics(listOf(prolonged))).lines.single() as KaraokeLine.MainKaraokeLine
        assertEquals("sai taaa", withCaption.phonetic)
        assertEquals(prolonged, withCaption.copy(phonetic = null))
    }
    @Test fun providedPhoneticsAndRegionalLanguageTags() {
        val source = line(listOf("银", "行")).copy(phonetic = "supplied")
        assertEquals(source, provider.enrich(SyncedLyrics(listOf(source))).lines.single())
        assertEquals("le se", provider.getPhonetic("垃圾", "zh-TW"))
        assertEquals("ngan hong", provider.getPhonetic("銀行", "yue-HK"))
    }
    @Test fun cedpaneAndMcBopomofoReadingsProjectOntoTimedSyllables() {
        for ((texts, language, expected) in listOf(
            Triple(listOf("沃", "兹", "沃", "思"), "zh-CN", listOf("wo", "zi", "wo", "si")),
            Triple(listOf("樂", "器"), "zh-TW", listOf("yue", "qi")),
        )) {
            val source = line(texts, language)
            val result = provider.enrich(SyncedLyrics(listOf(source))).lines.single() as KaraokeLine.MainKaraokeLine
            assertEquals(expected, result.syllables.map { it.phonetic })
            assertEquals(source.syllables.map { it.start to it.end }, result.syllables.map { it.start to it.end })
            assertEquals(source.syllables.map { it.content }, result.syllables.map { it.content })
            assertEquals(source.translation, result.translation)
        }
    }
    @Test fun syncedLinesAndLrcReceiveLineCaptionsWithoutInventingWordTiming() {
        val source = SyncedLine("垃圾", "translation", 100, 800, languageTag = "zh-TW")
        val lyrics = SyncedLyrics(listOf(source), title = "title", id = "id")
        val result = provider.enrich(lyrics)
        val converted = result.lines.single() as SyncedLine
        assertEquals("le se", converted.phonetic)
        assertEquals(source, converted.copy(phonetic = null))
        assertEquals(lyrics.copy(lines = listOf(converted)), result)
        assertEquals(result, provider.enrich(result))
        assertEquals("supplied", (provider.enrich(SyncedLyrics(listOf(source.copy(phonetic = "supplied")))).lines.single() as SyncedLine).phonetic)
        val parsed = AutoParser(fallbackPhoneticProvider = provider).parse("[00:01.00]银行行走\n[00:03.00]你好")
        assertEquals("yin hang xing zou", (parsed.lines.first() as SyncedLine).phonetic)
        assertEquals(1000, parsed.lines.first().start)
    }
    @Test fun mixedScriptsUseSeparateProfilesAndRetainChineseContext() {
        val source = line(listOf("银", "行", "한국", "hola canción", "日", "本", "語"), language = null).copy(
            syllables = listOf("银" to "zh-CN", "行" to "zh-CN", "한국" to "ko", "hola canción" to "es", "日" to "ja", "本" to "ja", "語" to "ja")
                .mapIndexed { index, (text, tag) -> KaraokeSyllable(text, index * 100, (index + 1) * 100, languageTag = tag) },
            languageTag = null,
        )
        val converted = provider.enrich(SyncedLyrics(listOf(source))).lines.single() as KaraokeLine
        assertEquals("yin hang hangug nihongo", converted.phonetic)
        assertEquals(source.syllables, converted.syllables, "Unknown Japanese character alignment must keep the original timing units")
        assertEquals("yin hang hangug", provider.getPhonetic("银行한국"))
        assertEquals("hangug yin hang", provider.getPhonetic("한국银行"))
        assertEquals("hangug", provider.getPhonetic("한국"))
    }
    @Test fun latinTextNeverGetsAnAutomaticallyGeneratedCaption() {
        for ((text, language) in listOf("hola canción" to "es", "français déjà" to "fr", "Hello World" to "en", "cafe\u0301" to null)) {
            val source = SyncedLine(text, null, 0, 100, languageTag = language)
            assertEquals(source, provider.enrich(SyncedLyrics(listOf(source))).lines.single())
            assertEquals("", provider.getPhonetic(text, language))
            val karaoke = line(listOf(text), language)
            assertEquals(karaoke, provider.enrich(SyncedLyrics(listOf(karaoke))).lines.single())
        }
        val mixed = line(listOf("你好", "hola canción", "한국"), language = null)
        val converted = provider.enrich(SyncedLyrics(listOf(mixed))).lines.single() as KaraokeLine
        assertNull(converted.phonetic, "Omitted Latin captions must not force a line fallback")
        assertEquals(listOf("ni hao", null, "hangug"), converted.syllables.map { it.phonetic })
        assertEquals(mixed.syllables.map { it.content }, converted.syllables.map { it.content })
        val provided = SyncedLine("hola", null, 0, 100, phonetic = "supplied", languageTag = "es")
        assertEquals(provided, provider.enrich(SyncedLyrics(listOf(provided))).lines.single())
    }
    @Test fun koreanCharacterFragmentsDoNotGuessWordAlignmentAndAccompanimentsAreEnriched() {
        val source = line(listOf("한", "국", "银", "行"), language = null)
        val converted = provider.enrich(SyncedLyrics(listOf(source))).lines.single() as KaraokeLine
        assertEquals("hangug yin hang", converted.phonetic)
        assertEquals(source.syllables, converted.syllables)
        val backing = KaraokeLine.AccompanimentKaraokeLine(listOf(KaraokeSyllable("你好", 0, 100)), null, KaraokeAlignment.Start, 0, 100)
        val supplied = source.copy(phonetic = "supplied", accompanimentLines = listOf(backing))
        val processed = provider.enrich(SyncedLyrics(listOf(supplied))).lines.single() as KaraokeLine.MainKaraokeLine
        assertEquals("supplied", processed.phonetic)
        assertEquals("ni hao", processed.accompanimentLines!!.single().syllables.single().phonetic)
    }

}
