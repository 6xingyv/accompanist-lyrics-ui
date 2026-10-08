package com.mocharealm.accompanist.lyrics.ui.internal.preparation

import com.ibm.icu.text.Transliterator
import com.mocharealm.accompanist.lyrics.core.model.karaoke.*
import com.mocharealm.accompanist.lyrics.core.parser.TTMLParser
import com.mocharealm.accompanist.lyrics.core.utils.PhoneticProvider
import com.mocharealm.accompanist.lyrics.ui.internal.test.*
import com.mocharealm.accompanist.lyrics.ui.internal.text.resolveProfiles
import com.mocharealm.accompanist.lyrics.ui.profile.*
import java.nio.file.Files
import java.nio.file.Path
import kotlin.test.*

class GroupedPhoneticTest {
    @Test
    fun projectedChineseBoundariesSurviveFinalCaptionPreparation() {
        val cases = listOf(
            Triple(listOf("为", "你", "着", "迷"), listOf("wei", "ni", "zhao", "mi"), "wei ni zhao mi"),
            Triple(listOf("你", "好"), listOf("nei", "hou"), "nei hou"),
            Triple(listOf("你好", "世界"), listOf("ni hao", "shi jie"), "ni hao shi jie"),
            Triple(listOf("着迷"), listOf("zhao mi"), "zhao mi"),
        )
        for ((parts, readings, expected) in cases) {
            val syllables = parts.mapIndexed { index, text ->
                KaraokeSyllable(text, index * 100, (index + 1) * 100, phonetic = readings[index],
                    phoneticSeparatorBefore = if (index == 0) "" else " ")
            }
            val units = CjkProfile.prepare(syllables, measurer, style)
            assertEquals(listOf(expected), units.mapNotNull { it.phonetic })
            val prepared = prepare(source(parts.joinToString("")).copy(syllables = syllables))
            val captions = prepared.rows.flatMap { it.runs }.flatMap { it.groups }.flatMap { it.units }.mapNotNull { it.phonetic }
            assertEquals(expected, captions.joinToString(" ") { it.layoutInput.text.text })
            assertEquals(units.map { it.timing }, CjkProfile.prepare(syllables.map { it.copy(phonetic = null) }, measurer, style).map { it.timing })
        }
        // A boundary belongs between captions; it does not pad the start of a new visual group.
        val syllables = listOf(KaraokeSyllable("银", 0, 100, "yin", phoneticSeparatorBefore = " "),
            KaraokeSyllable("，", 100, 200), KaraokeSyllable("行", 200, 300, "hang", phoneticSeparatorBefore = " "))
        assertEquals(listOf("yin hang"), CjkProfile.prepare(syllables, measurer, style).mapNotNull { it.phonetic })
    }

    @Test
    fun legacyJapaneseHanFragmentsStillConcatenate() {
        val syllables = listOf(KaraokeSyllable("今", 0, 100, "kyo", languageTag = "ja"),
            KaraokeSyllable("日", 100, 200, "u", languageTag = "ja"))
        assertEquals(listOf("kyou"), CjkProfile.prepare(syllables, measurer, style).mapNotNull { it.phonetic })
        val prepared = prepare(source("今日").copy(syllables = syllables))
        assertEquals(listOf("kyou"), prepared.rows.flatMap { it.runs }.flatMap { it.groups }.flatMap { it.units }
            .mapNotNull { it.phonetic }.map { it.layoutInput.text.text })
    }

    @Test
    fun splitWordsHaveOneJoinedPronunciationAcrossWritingSystems() {
        val cases = listOf(
            Triple(listOf("to", "night"), listOf("to", "night"), LatinProfile),
            Triple(listOf("para", "graphs"), listOf("para", "graphs"), LatinProfile),
            Triple(listOf("bon", "jour"), listOf("bon", "jour"), LatinProfile),
            Triple(listOf("при", "вет"), listOf("pri", "vet"), CyrillicProfile),
            Triple(listOf("κα", "λη"), listOf("ka", "li"), GreekProfile),
            Triple(listOf("س", "لام"), listOf("sa", "lam"), ArabicProfile),
            Triple(listOf("ש", "לום"), listOf("sha", "lom"), HebrewProfile),
            Triple(listOf("न", "मस्ते"), listOf("na", "maste"), IndicProfile),
            Triple(listOf("ส", "วัสดี"), listOf("sa", "watdi"), SoutheastAsianProfile),
            Triple(listOf("བོ", "ད"), listOf("bo", "d"), TibetanProfile),
            Triple(listOf("하", "ᆫ"), listOf("ha", "n"), HangulJamoProfile),
            Triple(listOf("𞤀", "𞤣"), listOf("a", "d"), FallbackProfile),
            Triple(listOf("生活"), listOf("sheng huo"), CjkProfile),
            Triple(listOf("きょう"), listOf("kyou"), CjkProfile),
            Triple(listOf("한글"), listOf("hangeul"), CjkProfile),
        )
        for ((parts, readings, profile) in cases) {
            val syllables = parts.mapIndexed { index, text ->
                KaraokeSyllable(text, 1000 + index * 1000, 2000 + index * 1000, phonetic = readings[index])
            }
            val units = profile.prepare(syllables, measurer, style)
            assertEquals(listOf(readings.joinToString("")), units.mapNotNull { it.phonetic }, "$parts profile preparation")
            val line = prepare(source(parts.joinToString("")).copy(syllables = syllables), profiles = listOf(profile))
            val captions = line.rows.flatMap { it.runs }.flatMap { it.groups }.flatMap { it.units }.mapNotNull { it.phonetic }
            assertEquals(listOf(readings.joinToString("")), captions.map { it.layoutInput.text.text }, "$parts prepared caption")
            assertEquals(units.map { it.animation }, profile.prepare(syllables.map { it.copy(phonetic = null) }, measurer, style).map { it.animation },
                "Pronunciation must not change original animation units")
        }
    }

    @Test
    fun sugarTalkingTtmlPreservesWholeWordReadingsFromSyllableFallback() {
        val transliterator = Transliterator.getInstance("Any-Latin; Latin-ASCII")
        val parser = TTMLParser(object : PhoneticProvider {
            override val phoneticLevel = PhoneticLevel.SYLLABLE
            override fun getPhonetic(string: String) = transliterator.transliterate(string).lowercase()
        })
        // Portable reproduction of the adjacent timed spans in the user's song.
        val fixture = """<tt xmlns="http://www.w3.org/ns/ttml" xml:lang="en"><body><div>
            <p begin="0" end="3"><span begin="0" end="1">to</span><span begin="1" end="3">night</span></p>
            <p begin="3" end="6"><span begin="3" end="4">para</span><span begin="4" end="6">graphs</span></p>
            <p begin="6" end="9"><span begin="6" end="7">no</span><span begin="7" end="9">thing?</span></p>
            </div></body></tt>"""
        val local = Path.of("C:/Users/Simon/Music/Local/Sabrina Carpenter/Man\u2019s Best Friend (Bonus Track Version)/04. Sugar Talking.ttml")
        val inputs = buildList {
            add(fixture)
            if (Files.isRegularFile(local)) add(Files.readString(local))
        }
        var splitWordCount = 0
        var actualSongWords = 0
        for (input in inputs) {
            for (line in parser.parse(input).lines.filterIsInstance<KaraokeLine>()) {
                val expectedReadings = mutableListOf<String>()
                for (run in resolveProfiles(line.syllables, DefaultLyricsProfiles)) {
                    if (run.profile !== LatinProfile) continue
                    for (group in run.groups) {
                        val units = run.profile.prepare(group, measurer, style)
                        val word = group.joinToString("") { it.content }.trim()
                        val captions = units.mapNotNull { it.phonetic }.map { it.trim() }
                        assertEquals(listOf(word.lowercase()), captions, "TTML word $word")
                        expectedReadings.add(word.lowercase())
                        if (input != fixture) actualSongWords++
                        if (group.count { it.content.isNotBlank() } > 1) splitWordCount++
                    }
                }
                val preparedReadings = prepare(line).rows.flatMap { it.runs }.flatMap { it.groups }
                    .flatMap { it.units }.mapNotNull { it.phonetic }.map { it.layoutInput.text.text.trim() }
                assertEquals(expectedReadings, preparedReadings, "Final prepared TTML captions")
            }
        }
        assertTrue(splitWordCount >= 3, "Must exercise the split words")
        println("Sugar Talking: checked $actualSongWords local-file word captions; $splitWordCount split words across inputs")
    }

    @Test
    fun widerJoinedReadingReservesSpaceBeforeTheNextWordForLtrAndRtl() {
        for (parts in listOf(listOf("to", "night ", "hello"), listOf("س", "لام ", "عالم"))) {
            val line = prepare(source(parts.joinToString("")).copy(
                syllables = parts.mapIndexed { index, text ->
                    KaraokeSyllable(text, index * 1000, (index + 1) * 1000,
                        phonetic = listOf("a considerably ", "wider pronunciation", "next")[index])
                },
            ), width = 900f)
            val groups = line.rows.single().runs.flatMap { it.groups }
            assertEquals(2, groups.size)
            val first = groups.first()
            val captions = first.units.mapNotNull { it.phonetic }
            assertEquals(1, captions.size)
            assertEquals("a considerably wider pronunciation", captions.single().layoutInput.text.text)
            assertEquals(captions.single().size.width.toFloat(), first.width, 0.01f)
            assertTrue(first.width > first.textWidth)
            fun start(group: com.mocharealm.accompanist.lyrics.ui.preparation.PreparedGroup): Float {
                val unit = group.units.first { it.phonetic != null }
                return unit.position.x + unit.phoneticPosition.x +
                    if (line.rows.single().rtl) unit.phonetic!!.size.width else 0
            }
            assertEquals(start(first) + (if (line.rows.single().rtl) -first.width else first.width),
                start(groups.last()), 0.01f)
        }
    }
}
