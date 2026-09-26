package com.mocharealm.accompanist.lyrics.ui.internal.text

import com.mocharealm.accompanist.lyrics.ui.preparation.MeasuredLyricsLine

import com.mocharealm.accompanist.lyrics.ui.internal.text.*
import com.mocharealm.accompanist.lyrics.ui.profile.*

import androidx.compose.ui.text.TextMeasurer
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.createFontFamilyResolver
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.ui.unit.sp
import com.mocharealm.accompanist.lyrics.core.model.karaoke.KaraokeSyllable
import kotlin.test.*

class ProfilePolicyTest {
    private val measurer =
        TextMeasurer(createFontFamilyResolver(), Density(1f), LayoutDirection.Ltr)
    private val style = TextStyle(fontSize = 30.sp)

    @Test
    fun cjkClustersSurviveSourceSyllableBoundaries() {
        for (parts in
            listOf(listOf("か", "\u3099"), listOf("ᄒ", "ᅡ", "ᆫ"), listOf("𠀀", "\uFE00"))) {
            val sources =
                parts.mapIndexed { i, part -> KaraokeSyllable(part, i * 100, (i + 1) * 100) }
            val group = CjkProfile.groups(sources).single()
            assertEquals(parts.joinToString(""), group.joinToString("") { it.content })
            val unit = CjkProfile.prepare(group, measurer, style).single()
            assertEquals(sources.map { it.start }, unit.timing.map { it.start })
        }
    }

    @Test
    fun longShapedContextsWrapWithoutLosingSourceRanges() {
        for (text in listOf("لا".repeat(120), "ffi".repeat(120), "e\u0301".repeat(120))) {
            // Supply ranges across UTF-16 and syllable boundaries, as a custom profile may do.
            val sources =
                text.mapIndexed { i, c -> KaraokeSyllable(c.toString(), i * 100, (i + 1) * 100) }
            val profile = WordLevelLyricsProfile()
            val wrapped =
                profile.wrap(profile.prepare(sources, measurer, style), measurer, 100f).flatten()
            assertTrue(wrapped.size > 1)
            assertEquals(
                text,
                wrapped.joinToString("") { unit ->
                    val range = assertNotNull(unit.sourceRange)
                    unit.layout.layoutInput.text.text.substring(range.min, range.max)
                },
            )
            assertTrue(wrapped.all { it.width <= 101f && it.height > 0f })
            assertEquals(0, wrapped.minOf { it.start })
            assertEquals(text.length * 100, wrapped.maxOf { it.end })
        }
    }

    @Test
    fun contextualScriptsRetainSyllablesAndSharedShaping() {
        val examples =
            listOf(
                ArabicProfile to "سلام",
                HebrewProfile to "שלום",
                IndicProfile to "नमस्ते",
                SoutheastAsianProfile to "ภาษาไทย",
                TibetanProfile to "བོད",
                HangulJamoProfile to "한",
                CyrillicProfile to "Привет",
                GreekProfile to "γειά",
                FallbackProfile to "🙂",
            )
        for ((profile, text) in examples) {
            val sources =
                listOf(KaraokeSyllable("$text $text", 100, 1500), KaraokeSyllable(text, 1500, 3000))
            val runs = resolveProfiles(sources, DefaultLyricsProfiles)
            assertEquals(1, runs.size, text)
            assertSame(profile, runs.single().profile, text)
            assertEquals(listOf(sources), runs.single().groups, text)
            val measured =
                MeasuredLyricsLine(
                    measurer.measure(
                        sources.joinToString("") { it.content },
                        style,
                        softWrap = false,
                    ),
                    sources,
                )
            val units = profile.prepare(measured, sources, measurer, style)
            assertTrue(units.size in 1..2, text)
            assertTrue(units.all { it.layout === measured.layout }, text)
            assertEquals(
                sources.map { it.start to it.end },
                units.flatMap { it.timing }.map { it.start to it.end },
                text,
            )
            if (units.size == 1)
                assertEquals(ProfileAnimationUnit(100, 3000), units.single().animation, text)
            else
                assertEquals(
                    sources.map { ProfileAnimationUnit(it.start, it.end) },
                    units.map { it.animation },
                    text,
                )
            assertEquals(ProfileGroupEffects(lift = true), profile.effects(units, false), text)
        }
    }

    @Test
    fun mixedRunsKeepPunctuationAndCallerOrdering() {
        val source = KaraokeSyllable("Ahhh, 我的生活! שלום Привет", 0, 12000)
        val runs = resolveProfiles(listOf(source), DefaultLyricsProfiles)
        assertEquals(
            listOf(LatinProfile, CjkProfile, HebrewProfile, CyrillicProfile),
            runs.map { it.profile },
        )
        assertEquals(
            source.content,
            runs.flatMap { it.groups }.flatten().joinToString("") { it.content },
        )
        val custom =
            object : WordLevelLyricsProfile() {
                override fun matches(syllable: KaraokeSyllable) = syllable.start == 100
            }
        val overridden =
            resolveProfiles(
                listOf(source.copy(start = 100)),
                listOf(custom) + DefaultLyricsProfiles,
            )
        assertEquals(1, overridden.size)
        assertSame(custom, overridden.single().profile)
        val reordered =
            resolveProfiles(listOf(source), listOf(FallbackProfile) + DefaultLyricsProfiles)
        assertEquals(1, reordered.size)
        assertSame(FallbackProfile, reordered.single().profile)
    }

    @Test
    fun indexedSourceLookupMatchesFirstContainingRange() {
        val layout = measurer.measure("x", style)
        val ordered = List(4096) { KaraokeSyllable("x", it * 10, it * 10 + 15) }
        for (sources in
            listOf(
                ordered,
                ordered.reversed(),
                listOf(
                    KaraokeSyllable("x", 0, 100),
                    KaraokeSyllable("x", 10, 20),
                    KaraokeSyllable("x", 20, 30),
                ),
            )) {
            val measured = MeasuredLyricsLine(layout, sources)
            for (index in sources.indices step 7) {
                val start = sources[index].start + 1
                val end = sources[index].end
                assertEquals(
                    measured.ranges.firstOrNull {
                        start >= it.source.start && end <= it.source.end
                    },
                    measured.sourceFor(start, end),
                )
            }
            assertNull(measured.sourceFor(-100, -90))
            assertNull(measured.sourceFor(100000, 100010))
        }
    }
}
