package com.mocharealm.accompanist.lyrics.ui.internal.preparation

import com.mocharealm.accompanist.lyrics.ui.internal.layout.*
import com.mocharealm.accompanist.lyrics.ui.internal.playback.*
import com.mocharealm.accompanist.lyrics.ui.internal.rendering.*
import com.mocharealm.accompanist.lyrics.ui.internal.text.*
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.*
import androidx.compose.ui.graphics.drawscope.CanvasDrawScope
import androidx.compose.ui.graphics.drawscope.translate
import androidx.compose.ui.text.TextMeasurer
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.drawText
import androidx.compose.ui.text.font.createFontFamilyResolver
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.ui.unit.sp
import com.mocharealm.accompanist.lyrics.core.model.SyncedLyrics
import com.mocharealm.accompanist.lyrics.core.model.karaoke.*
import com.mocharealm.accompanist.lyrics.ui.internal.playback.BoundaryCursor
import com.mocharealm.accompanist.lyrics.ui.internal.playback.LyricsPlaybackTimeline
import com.mocharealm.accompanist.lyrics.ui.preparation.PreparedLyrics
import com.mocharealm.accompanist.lyrics.ui.preparation.prepareLyrics
import com.mocharealm.accompanist.lyrics.ui.preparation.prepareLyricsLine
import com.mocharealm.accompanist.lyrics.ui.profile.ArabicProfile
import com.mocharealm.accompanist.lyrics.ui.profile.CjkProfile
import com.mocharealm.accompanist.lyrics.ui.profile.FallbackProfile
import com.mocharealm.accompanist.lyrics.ui.profile.DefaultLyricsProfiles
import com.mocharealm.accompanist.lyrics.ui.profile.LatinProfile
import com.mocharealm.accompanist.lyrics.ui.profile.LyricsProfile
import com.mocharealm.accompanist.lyrics.ui.preparation.MeasuredLyricsLine
import com.mocharealm.accompanist.lyrics.ui.profile.ProfileGroupEffects
import com.mocharealm.accompanist.lyrics.ui.profile.ProfileTextUnit
import com.mocharealm.accompanist.lyrics.ui.profile.DefaultLyricsProfile
import com.mocharealm.accompanist.lyrics.ui.internal.rendering.RowPaints
import com.mocharealm.accompanist.lyrics.ui.internal.rendering.LyricsRenderResources
import com.mocharealm.accompanist.lyrics.ui.internal.rendering.RowRenderState
import com.mocharealm.accompanist.lyrics.ui.internal.rendering.drawPreparedRow
import com.mocharealm.accompanist.lyrics.ui.internal.rendering.prepareLineRaster
import kotlin.test.*
import com.mocharealm.accompanist.lyrics.ui.internal.test.*

class LyricsPreparationTest {
    @Test
    fun animatedUnitsPreserveTheOriginalShapedPixels() {
        val font =
            androidx.compose.ui.text.platform.Font(
                java.io.File("../sample/shared/src/commonMain/composeResources/font/sf_pro.ttf")
            )
        val glyphStyle =
            TextStyle(fontFamily = androidx.compose.ui.text.font.FontFamily(font), fontSize = 64.sp)
        for (text in
            listOf("Yo", "yo", "AV", "To", "office", "e\u0301", "لا", "क्\u0937", "👩‍👩‍👧‍👦")) {
            val profile = com.mocharealm.accompanist.lyrics.ui.profile.WordLevelLyricsProfile()
            val sources =
                text.mapIndexed { i, c -> KaraokeSyllable(c.toString(), i * 100, (i + 1) * 100) }
            val units = profile.prepare(sources, measurer, glyphStyle)
            val whole = measurer.measure(text, glyphStyle, softWrap = false)
            fun image(draw: androidx.compose.ui.graphics.drawscope.DrawScope.() -> Unit): IntArray {
                val width = whole.size.width + 48
                val height = whole.size.height + 48
                val bitmap = ImageBitmap(width, height)
                CanvasDrawScope().draw(
                    Density(1f),
                    LayoutDirection.Ltr,
                    Canvas(bitmap),
                    Size(width.toFloat(), height.toFloat()),
                ) {
                    translate(24f, 24f, draw)
                }
                return IntArray(width * height).also { bitmap.readPixels(it) }
            }
            val actual = image {
                for (unit in units) translate(unit.left, 0f) {
                    with(profile) { draw(unit, Color.White, Shadow.None) }
                }
            }
            val expected = image { drawText(whole, Color.White) }
            assertContentEquals(expected, actual, "$text must retain its whole shaped glyphs")
            assertEquals(sources.size, units.sumOf { it.timing.size })
            if (text in listOf("Yo", "e\u0301", "لا", "👩‍👩‍👧‍👦"))
                assertEquals(
                    1,
                    units.size,
                    "$text cannot be cut into independently transformed slices",
                )
        }
    }

    @Test
    fun sweepFadeWidthUsesTwoEmForSampleSfPro() {
        val font =
            androidx.compose.ui.text.platform.Font(
                java.io.File("../sample/shared/src/commonMain/composeResources/font/sf_pro.ttf")
            )
        for (size in listOf(20, 34)) {
            val sf =
                TextStyle(
                    fontFamily = androidx.compose.ui.text.font.FontFamily(font),
                    fontSize = size.sp,
                )
            val line =
                prepareLyricsLine(
                    source("Hello", 1000, 4000),
                    DefaultLyricsProfiles,
                    measurer,
                    sf,
                    sf,
                    sf,
                    500f,
                    1f,
                    false,
                    translationStyle = androidx.compose.ui.text.TextStyle(),
                )
            assertEquals(size * 2f, line.rows.single().sweepFadeWidth, 0.001f)
        }
    }

    @Test
    fun translationAndPhoneticUseIndependentTextStyles() {
        val phoneticStyle = TextStyle(fontSize = 9.sp)
        val translationStyle = TextStyle(fontSize = 23.sp)
        val line =
            prepareLyricsLine(
                source("Hello").copy(
                    translation = "translated",
                    syllables = listOf(KaraokeSyllable("Hello", 1000, 3000, phonetic = "phonetic")),
                ),
                DefaultLyricsProfiles,
                measurer,
                style,
                style,
                phoneticStyle,
                500f,
                1f,
                true,
                translationStyle = translationStyle,
            )

        assertEquals(23.sp, line.translation!!.layoutInput.style.fontSize)
        val phonetic =
            line.rows.single().runs.flatMap { it.groups }.flatMap { it.units }
                .firstNotNullOf { it.phonetic }
        assertEquals(9.sp, phonetic.layoutInput.style.fontSize)
    }

    @Test
    fun absentAndBlankCaptionsDoNotReserveLineHeight() {
        for (caption in listOf(null, "", "  \n\t")) {
            val child =
                KaraokeLine.AccompanimentKaraokeLine(
                    listOf(KaraokeSyllable("echo", 0, 1000, phonetic = caption)),
                    caption,
                    KaraokeAlignment.Start,
                    0,
                    1000,
                )
            val line =
                prepare(
                    source("hello").copy(translation = caption, accompanimentLines = listOf(child))
                )
            for (prepared in listOf(line) + line.before + line.after) {
                assertNull(prepared.translation)
                assertEquals(
                    prepared.rows.sumOf { it.height.toDouble() }.toFloat(),
                    prepared.height,
                )
                for (row in prepared.rows) assertEquals(0f, row.phoneticHeight)
            }
        }
        assertNotNull(prepare(source("hello").copy(translation = "你好")).translation)
    }

    @Test
    fun emergencyWrappingPreservesTextAndShapedLineSlices() {
        for (text in
            listOf(
                "العربية".repeat(20),
                "office".repeat(20),
                "देवनागरी".repeat(20),
                "🙂".repeat(20),
            )) {
            val profile =
                DefaultLyricsProfiles.first { it.matches(KaraokeSyllable(text, 1000, 9000)) }
            val original =
                profile.prepare(listOf(KaraokeSyllable(text, 1000, 9000)), measurer, style)
            val wrapped = profile.wrap(original, measurer, 100f).flatten()
            assertTrue(wrapped.size > 1)
            val restored =
                wrapped.joinToString("") {
                    val text = it.layout.layoutInput.text.text
                    it.sourceRange?.let { range -> text.substring(range.min, range.max) } ?: text
                }
            assertEquals(text, restored)
            assertTrue(wrapped.all { it.layout.lineCount == 1 && !it.layout.isLineEllipsized(0) })
            assertTrue(wrapped.all { it.height > 0f && it.baseline > 0f })
            assertTrue(wrapped.all { it.timing.isNotEmpty() })
        }
    }

    @Test
    fun wrappedFragmentsUseTheSameMetricsAsIndependentUnits() {
        val cases = listOf(
            FallbackProfile to "العربية".repeat(12),
            LatinProfile to "office".repeat(12),
            FallbackProfile to "🙂".repeat(12),
        )
        for ((profile, text) in cases) {
            for (lineHeight in listOf(androidx.compose.ui.unit.TextUnit.Unspecified, 48.sp)) {
                val typography = style.copy(lineHeight = lineHeight)
                // Exercise emergency wrapping of one oversized shaped unit, independently of
                // the profile's normal segmentation policy (which may already split this text).
                val prepared =
                    DefaultLyricsProfile()
                        .prepare(listOf(KaraokeSyllable(text, 0, 10000)), measurer, typography)
                val wrapped = profile.wrap(prepared, measurer, 100f).flatten()
                assertTrue(wrapped.size > 1)
                for (fragment in wrapped) {
                    val ordinary =
                        DefaultLyricsProfile()
                            .prepare(
                                listOf(
                                    KaraokeSyllable(
                                        fragment.layout.layoutInput.text.text,
                                        fragment.start,
                                        fragment.end,
                                    )
                                ),
                                measurer,
                                typography,
                            )
                            .single()
                    assertEquals(1, fragment.layout.lineCount)
                    assertEquals(ordinary.height, fragment.height)
                    assertEquals(ordinary.baseline, fragment.baseline)
                    assertEquals(ordinary.width, fragment.width)
                }
            }
        }
    }

    @Test
    fun cjkRowGeometryDoesNotDependOnSyllableBoundaries() {
        val text = "我的生活不安です".repeat(3)
        val whole = prepare(source(text), width = 100f)
        val split =
            prepare(
                source(text)
                    .copy(
                        syllables =
                            text.mapIndexed { index, value ->
                                KaraokeSyllable(
                                    value.toString(),
                                    1000 + index * 100,
                                    1100 + index * 100,
                                )
                            }
                    ),
                width = 100f,
            )
        assertEquals(whole.rows.map { it.top to it.height }, split.rows.map { it.top to it.height })
        assertEquals(whole.height, split.height)
        val wholePositions =
            whole.rows
                .flatMap { it.runs }
                .flatMap { it.groups }
                .flatMap { it.units }
                .map { it.position }
        val splitPositions =
            split.rows
                .flatMap { it.runs }
                .flatMap { it.groups }
                .flatMap { it.units }
                .map { it.position }
        assertEquals(wholePositions, splitPositions)
    }

    @Test
    fun profilesChooseEffectsAndIndependentLiftUnits() {
        val latin = prepare(source("Ahhh This", 1000, 9000))
        val latinGroups = latin.runs.flatMap { it.groups }
        assertTrue(latinGroups.all { it.effects.scale && it.effects.glow && it.effects.lift })
        val latinUnits = latinGroups.flatMap { it.units }
        assertTrue(latinUnits.size > 1)
        assertTrue(
            latinUnits.all { it.animation.timing.start == 1000 && it.animation.timing.end == 9000 }
        )
        val cjk = prepare(source("生活", 1000, 7000))
        val cjkGroups = cjk.runs.flatMap { it.groups }
        assertTrue(cjkGroups.all { !it.effects.scale && it.effects.glow && it.effects.lift })
        assertEquals(
            listOf(1000, 4000),
            cjkGroups.flatMap { it.animationUnits }.map { it.timing.start },
        )
        val arabic =
            prepare(
                source("سلام")
                    .copy(
                        syllables =
                            listOf(
                                KaraokeSyllable("سل", 1000, 1800),
                                KaraokeSyllable("ام", 1800, 3000),
                            )
                    )
            )
        assertEquals(
            listOf(1000),
            arabic.runs.flatMap { it.groups }.flatMap { it.animationUnits }.map { it.timing.start },
        )
        val noEffects =
            object : DefaultLyricsProfile() {
                override fun effects(units: List<ProfileTextUnit>, accompaniment: Boolean) =
                    ProfileGroupEffects(lift = false)
            }
        assertTrue(
            prepare(source("test", 0, 9000), profiles = listOf(noEffects))
                .runs
                .flatMap { it.groups }
                .all { !it.awesome && !it.effects.lift }
        )
    }

    @Test
    fun profilesReceiveInitialWholeLineMeasurementAndSyllableRanges() {
        var received: MeasuredLyricsLine? = null
        val profile =
            object : DefaultLyricsProfile() {
                override fun prepare(
                    line: MeasuredLyricsLine,
                    group: List<KaraokeSyllable>,
                    measurer: TextMeasurer,
                    style: TextStyle,
                ): List<ProfileTextUnit> {
                    received = line
                    return super.prepare(line, group, measurer, style)
                }
            }
        val text = "abcdefghijklmnop".repeat(4)
        prepare(source(text), 100f, listOf(profile))
        val measured = assertNotNull(received)
        assertEquals(text, measured.layout.layoutInput.text.text)
        assertTrue(measured.layout.size.width > 100)
        assertEquals(0, measured.ranges.single().textStart)
        assertEquals(text.length, measured.ranges.single().textEnd)
    }

    @Test
    fun profilesKeepOversizedContentInsideRows() {
        for (text in
            listOf(
                "我的生活".repeat(20),
                "abcdefgh".repeat(20),
                "العربية".repeat(20),
                "देवनागरी".repeat(20),
                "🙂".repeat(20),
            )) {
            val line = prepare(source(text), width = 100f)
            assertTrue(line.rows.isNotEmpty())
            for (row in line.rows) for (run in row.runs) for (group in run.groups) {
                assertTrue(group.width <= 100f, "$text group width ${group.width}")
                for (unit in group.units) {
                    assertTrue(unit.position.x >= -0.01f)
                    assertTrue(unit.position.x + unit.width <= 100.01f)
                }
            }
            if (text.startsWith("我的") || text.startsWith("abc")) assertTrue(line.rows.size > 1)
        }
    }

    @Test
    fun contextualSyllableMovesIntactToNextRow() {
        val syllable = KaraokeSyllable("مرحبا بكم", 2000, 3000)
        val units = FallbackProfile.prepare(listOf(syllable), measurer, style)
        val width = units.single().width + 5f
        val line =
            prepare(
                source("Hello ")
                    .copy(syllables = listOf(KaraokeSyllable("Hello ", 1000, 2000), syllable)),
                width = width,
            )
        assertEquals(1, FallbackProfile.groups(listOf(syllable)).size)
        val contextualRows =
            line.rows.filter { row -> row.runs.any { it.profile === ArabicProfile } }
        assertEquals(1, contextualRows.size)
        assertTrue(contextualRows.single() !== line.rows.first())
        val unit = contextualRows.single().runs.single().groups.single().units.single()
        assertEquals(syllable.content, unit.text.layout.layoutInput.text.text)
        assertFalse(unit.text.layout.isLineEllipsized(0))
    }

    @Test
    fun duetUsesEightyPercentWidthWhileKeepingBothOuterEdges() {
        val text = "Alpha Beta Gamma Delta Epsilon Zeta Eta Theta"
        val left = source(text)
        val right = source(text).copy(alignment = KaraokeAlignment.End)
        val prepared =
            prepareLyrics(
                SyncedLyrics(listOf(left, right)),
                DefaultLyricsProfiles,
                measurer,
                style,
                style,
                style,
                500f,
                1f,
                false,
                translationStyle = androidx.compose.ui.text.TextStyle(),
            )
        for (line in prepared.lines.filterNotNull()) for (row in line.rows) {
            val groups = row.runs.flatMap { it.groups }
            assertTrue(groups.sumOf { it.width.toDouble() } <= 400.01)
            val units = groups.flatMap { it.units }
            if (line.rightAligned) {
                assertTrue(units.minOf { it.position.x } >= 99.99f)
                assertEquals(500f, units.maxOf { it.position.x + it.width }, 0.01f)
            } else assertEquals(0f, units.minOf { it.position.x }, 0.01f)
            assertTrue(
                units.all {
                    it.text.layout.layoutInput.style.textMotion ==
                        androidx.compose.ui.text.style.TextMotion.Animated
                }
            )
        }
    }

    @Test
    fun rightAlignedWrappingCarriesSeparatorsToNextRow() {
        val profile = DefaultLyricsProfile()
        val text = "Alpha  Beta Gamma"
        val width =
            listOf("Alpha", "  Beta", " Gamma")
                .maxOf { measurer.measure(it, style, softWrap = false).size.width }
                .toFloat() + 1f
        val line =
            prepare(source(text).copy(alignment = KaraokeAlignment.End), width, listOf(profile))
        val rows =
            line.rows.map { row ->
                row.runs
                    .flatMap { it.groups }
                    .joinToString("") { it.units.single().text.layout.layoutInput.text.text }
            }
        assertEquals(listOf("Alpha", "  Beta", " Gamma"), rows)
        assertEquals(text, rows.joinToString(""))
        for (row in line.rows) {
            val unit = row.runs.last().groups.last().units.last()
            assertEquals(width, unit.position.x + unit.width, 0.01f)
        }
        val timing =
            line.rows.flatMap { row ->
                row.runs.flatMap { it.groups }.flatMap { it.units }.flatMap { it.text.timing }
            }
        assertEquals(1000, timing.first().start)
        assertEquals(3000, timing.last().end)
        for ((a, b) in timing.zipWithNext()) assertEquals(a.end, b.start)
    }

    @Test
    fun leadingWhitespaceAssignmentPreservesMixedProfilesAndSourceTiming() {
        val source = listOf(KaraokeSyllable("Hello  我的生活", 0, 1100))
        val runs = resolveProfiles(source, DefaultLyricsProfiles, leadingWhitespace = true)
        assertEquals(listOf(LatinProfile, CjkProfile), runs.map { it.profile })
        assertEquals("Hello", runs.first().groups.single().joinToString("") { it.content })
        assertTrue(runs.last().groups.first().first().content.startsWith("  "))
        val fragments = runs.flatMap { it.groups }.flatten()
        assertEquals(source.single().content, fragments.joinToString("") { it.content })
        for ((a, b) in fragments.zipWithNext()) assertEquals(a.end, b.start)
        val ordinary = resolveProfiles(source, DefaultLyricsProfiles)
        assertEquals("Hello  ", ordinary.first().groups.single().joinToString("") { it.content })
    }

    @Test
    fun mixedProfilesRetainRunsAcrossNeutralSyllables() {
        val syllables =
            listOf("Ahhh ", "This ", "is ", "我的生活", " ", "不安です", "!").mapIndexed { i, s ->
                KaraokeSyllable(s, i * 100, (i + 1) * 100)
            }
        val runs = resolveProfiles(syllables, DefaultLyricsProfiles)
        assertEquals(listOf(LatinProfile, CjkProfile), runs.map { it.profile })
        assertEquals(
            syllables.joinToString("") { it.content },
            runs.joinToString("") {
                it.groups.joinToString("") { it.joinToString("") { it.content } }
            },
        )
        assertEquals(
            listOf(LatinProfile, CjkProfile),
            resolveProfiles(
                    listOf(KaraokeSyllable("Ahhh This is 我的生活 不安です", 0, 2000)),
                    DefaultLyricsProfiles,
                )
                .map { it.profile },
        )
        val override = object : DefaultLyricsProfile() {}
        assertSame(
            override,
            resolveProfiles(syllables, listOf(override) + DefaultLyricsProfiles)
                .single()
                .profile,
        )
    }

    @Test
    fun shapingIsSharedAndGeometrySurvivesWrapping() {
        val line = prepare(source("Supercalifragilisticexpialidocious"), 100f)
        assertTrue(line.rows.size > 1)
        val groups = line.runs.flatMap { it.groups }
        assertEquals(groups.size, line.rows.sumOf { it.runs.sumOf { it.groups.size } })
        for (row in line.rows) for (run in row.runs) for (group in run.groups) {
            assertTrue(groups.any { it === group })
            assertTrue(group.width <= 101f)
            for (unit in group.units) assertTrue(unit.position.x + unit.width <= 101f)
        }
        val units = groups.flatMap { it.units }
        assertTrue(units.all { it.text.layout === units.first().text.layout })
    }

    @Test
    fun contextualShapingKeepsSyllableTimingAndUnicodeSequences() {
        val line =
            source("سلام")
                .copy(
                    syllables =
                        listOf(KaraokeSyllable("سل", 1000, 1800), KaraokeSyllable("ام", 1800, 3000))
                )
        val prepared = prepare(line)
        // A joining glyph crossing the syllable boundary must move as one drawable.
        assertEquals(1, prepared.runs.single().groups.single().units.size)
        assertEquals(
            listOf(1000, 1800),
            prepared.runs.single().groups.single().units.single().text.timing.map { it.start },
        )
        assertContentEquals(intArrayOf(1000, 1800), prepared.rows.single().sweepStarts)
        val emoji = prepare(source("👩‍👩‍👧‍👦"))
        assertEquals(1, emoji.runs.single().groups.single().units.size)
        val combining = prepare(source("e\u0301"))
        assertEquals(1, combining.runs.single().groups.single().units.size)
    }
}
