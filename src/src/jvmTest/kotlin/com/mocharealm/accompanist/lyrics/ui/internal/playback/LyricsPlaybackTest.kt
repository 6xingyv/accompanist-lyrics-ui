package com.mocharealm.accompanist.lyrics.ui.internal.playback

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

class LyricsPlaybackTest {
    @Test
    fun earlyAccompanimentPrecedesMainTextEvenWhenContainerStartsEarlier() {
        val early =
            KaraokeLine.AccompanimentKaraokeLine(
                listOf(KaraokeSyllable("intro", 1000, 1500)),
                null,
                KaraokeAlignment.Start,
                1000,
                1500,
            )
        val late =
            early.copy(
                syllables = listOf(KaraokeSyllable("echo", 2500, 3500)),
                start = 2500,
                end = 3500,
            )
        val source =
            source("main", 2000, 4000).copy(start = 1000, accompanimentLines = listOf(late, early))
        val line = prepare(source)
        assertEquals(early, line.before.single().source)
        assertEquals(late, line.after.single().source)
        val timeline =
            LyricsPlaybackTimeline(SyncedLyrics(listOf(source)), PreparedLyrics(listOf(line)))
        timeline.update(1100)
        assertTrue(timeline.state.line(line.before.single()).visible.value)
        assertTrue(0 in timeline.focus.value.allIndices)
        assertEquals(1100, timeline.state.row(line.before.single().rows.single()).time.intValue)
    }

    @Test
    fun framesNeverPrepareAgainAndStaticRowsStopObservingTime() {
        var preparations = 0
        val profile =
            object : DefaultLyricsProfile() {
                override fun prepare(
                    line: MeasuredLyricsLine,
                    group: List<KaraokeSyllable>,
                    measurer: TextMeasurer,
                    style: TextStyle,
                ): List<ProfileTextUnit> {
                    preparations++
                    return super.prepare(line, group, measurer, style)
                }
            }
        val sources = listOf(source("first", 1000, 2000), source("second", 5000, 6000))
        val prepared = PreparedLyrics(sources.map { prepare(it, profiles = listOf(profile)) })
        val timeline = LyricsPlaybackTimeline(SyncedLyrics(sources), prepared)
        val count = preparations
        timeline.update(0)
        val future = timeline.state.row(prepared.rows.last()).time.intValue
        for (time in 1000..2400 step 16) timeline.update(time)
        assertEquals(future, timeline.state.row(prepared.rows.last()).time.intValue)
        timeline.update(3000)
        val finished = timeline.state.row(prepared.rows.first()).time.intValue
        for (time in 3016..4500 step 16) timeline.update(time)
        assertEquals(finished, timeline.state.row(prepared.rows.first()).time.intValue)
        timeline.update(6500)
        assertEquals(Int.MAX_VALUE, timeline.state.row(prepared.rows.last()).time.intValue)
        timeline.update(0)
        assertEquals(Int.MIN_VALUE, timeline.state.row(prepared.rows.last()).time.intValue)
        assertEquals(count, preparations)
    }

    @Test
    fun accompanimentVisibilitySpansMainAndKeepsItsCorner() {
        for (alignment in listOf(KaraokeAlignment.Start, KaraokeAlignment.End)) {
            for (start in listOf(500, 1000, 2000)) {
                for (end in listOf(2500, 4000)) {
                    val acc =
                        KaraokeLine.AccompanimentKaraokeLine(
                            listOf(KaraokeSyllable("echo", start, end)),
                            null,
                            alignment,
                            start,
                            end,
                        )
                    val main =
                        source("main", 1000, 3000)
                            .copy(alignment = alignment, accompanimentLines = listOf(acc))
                    val line = prepare(main)
                    val nested = (line.before + line.after).single()
                    val timeline =
                        LyricsPlaybackTimeline(
                            SyncedLyrics(listOf(main)),
                            PreparedLyrics(listOf(line)),
                        )
                    val show = minOf(start, 1000)
                    val hide = maxOf(end, 3000)
                    assertEquals(start < 1000, nested.revealFromBottom)
                    assertEquals(alignment == KaraokeAlignment.End, nested.rightAligned)
                    timeline.update(show - 1)
                    assertFalse(timeline.state.line(nested).visible.value)
                    timeline.update(show)
                    assertTrue(timeline.state.line(nested).visible.value)
                    timeline.update(hide - 1)
                    assertTrue(timeline.state.line(nested).visible.value)
                    timeline.update(hide)
                    assertFalse(timeline.state.line(nested).visible.value)
                    timeline.update(show)
                    assertTrue(
                        timeline.state.line(nested).visible.value,
                        "Seeking back must restore the shared interval",
                    )
                }
            }
        }
    }

    @Test
    fun accompanimentIsPreparedBeforeVisibilityAndSurvivesSeeks() {
        val accompaniment =
            KaraokeLine.AccompanimentKaraokeLine(
                listOf(KaraokeSyllable("echo", 8000, 9000)),
                null,
                KaraokeAlignment.Start,
                8000,
                9000,
            )
        val source = source("main").copy(accompanimentLines = listOf(accompaniment))
        val line = prepare(source)
        val nested = line.after.single()
        val timeline =
            LyricsPlaybackTimeline(SyncedLyrics(listOf(source)), PreparedLyrics(listOf(line)))
        timeline.update(0)
        assertFalse(timeline.state.line(nested).visible.value)
        timeline.update(8100)
        assertTrue(timeline.state.line(nested).visible.value)
        assertEquals(8100, timeline.state.row(nested.rows.single()).time.intValue)
        timeline.update(10000)
        assertFalse(timeline.state.line(nested).visible.value)
        timeline.update(8500)
        assertTrue(timeline.state.line(nested).visible.value)
        assertSame(nested, line.after.single())
    }

    @Test
    fun nestedAndTopLevelAccompanimentShareOnePreparedLine() {
        val accompaniment =
            KaraokeLine.AccompanimentKaraokeLine(
                listOf(KaraokeSyllable("echo", 500, 1500)),
                null,
                KaraokeAlignment.Start,
                500,
                1500,
            )
        val source = source("main").copy(accompanimentLines = listOf(accompaniment))
        val lyrics = SyncedLyrics(listOf(source, accompaniment))
        val prepared =
            prepareLyrics(
                lyrics,
                DefaultLyricsProfiles,
                measurer,
                style,
                style,
                style,
                500f,
                1f,
                true,
            )
        assertSame(prepared.lines[0]!!.before.single(), prepared.lines[1])
        assertEquals(prepared.allLines.sumOf { it.rows.size }, prepared.rows.size)
        val timeline = LyricsPlaybackTimeline(lyrics, prepared)
        timeline.update(600)
        assertTrue(0 in timeline.focus.value.allIndices)
        assertEquals(0, timeline.focus.value.firstIndex)
        val focusBeforeEnd = timeline.focus.value
        val heightBeforeEnd = prepared.lines[0]!!.settledHeight(timeline.state, true, true, 1f)
        timeline.update(1501)
        assertTrue(timeline.state.line(prepared.lines[1]!!).visible.value)
        assertEquals(focusBeforeEnd, timeline.focus.value)
        assertEquals(heightBeforeEnd, prepared.lines[0]!!.settledHeight(timeline.state, true, true, 1f))
    }

    @Test
    fun overlappingFocusAndInterludesMatchIntervalsAcrossSeeks() {
        val sources =
            listOf(
                source("one", 6000, 9000),
                source("two", 8000, 10000),
                source("three", 18000, 20000),
            )
        val lyrics = SyncedLyrics(sources)
        val timeline = LyricsPlaybackTimeline(lyrics, PreparedLyrics(sources.map { prepare(it) }))
        timeline.update(0)
        assertTrue(timeline.focus.value.activeIntro)
        timeline.update(8500)
        assertEquals(listOf(0, 1), timeline.focus.value.allIndices)
        timeline.update(9000)
        assertEquals(listOf(1), timeline.focus.value.allIndices)
        timeline.update(12000)
        assertEquals(2, timeline.focus.value.activeInterludeIndex)
        timeline.update(8500)
        assertEquals(listOf(0, 1), timeline.focus.value.allIndices)
        assertNull(timeline.focus.value.activeInterludeIndex)
    }

    @Test
    fun cursorHandlesForwardPlaybackDuplicateBoundariesAndSeeks() {
        val cursor = BoundaryCursor(intArrayOf(10, 20, 20, 40))
        cursor.advance(0)
        assertEquals(-1, cursor.index)
        cursor.advance(20)
        assertEquals(2, cursor.index)
        assertFalse(cursor.advance(21))
        cursor.advance(100000)
        assertEquals(3, cursor.index)
        cursor.advance(15)
        assertEquals(0, cursor.index)
    }

    @Test
    fun longPauseWithinRowDoesNotTickAndForwardSkipsFinishRows() {
        val source =
            source("a ")
                .copy(
                    syllables =
                        listOf(KaraokeSyllable("a ", 1000, 1200), KaraokeSyllable("b", 8000, 8200)),
                    end = 8200,
                )
        val line = prepare(source)
        val timeline =
            LyricsPlaybackTimeline(SyncedLyrics(listOf(source)), PreparedLyrics(listOf(line)))
        timeline.update(0)
        timeline.update(1900)
        val frozen = timeline.state.row(line.rows.single()).time.intValue
        for (time in 1916..7000 step 16) timeline.update(time)
        assertEquals(frozen, timeline.state.row(line.rows.single()).time.intValue)
        timeline.update(8100)
        assertEquals(8100, timeline.state.row(line.rows.single()).time.intValue)

        val shortSources = listOf(source("x", 100, 120), source("y", 300, 320))
        val shortPrepared = PreparedLyrics(shortSources.map { prepare(it) })
        val shortTimeline = LyricsPlaybackTimeline(SyncedLyrics(shortSources), shortPrepared)
        shortTimeline.update(0)
        shortTimeline.update(900)
        assertEquals(Int.MAX_VALUE, shortTimeline.state.row(shortPrepared.rows.first()).time.intValue)
        shortTimeline.update(1100)
        assertEquals(Int.MAX_VALUE, shortTimeline.state.row(shortPrepared.rows.last()).time.intValue)
    }

    @Test
    fun mixedRtlDoesNotReverseLatinShapingUnits() {
        val line =
            source("مرحبا Hello")
                .copy(
                    syllables =
                        listOf(
                            KaraokeSyllable("مرحبا ", 1000, 2000),
                            KaraokeSyllable("Hello", 2000, 4000),
                        ),
                    end = 4000,
                )
        val prepared = prepare(line)
        assertTrue(prepared.rightAligned)
        val latin = prepared.runs.last().groups.single().units
        for (index in 1 until latin.size) assertTrue(
            latin[index].position.x >= latin[index - 1].position.x
        )
        assertFalse(prepare(source("Hello مرحبا")).rightAligned)
    }

    @Test
    fun zeroDurationAndEmptyLyricsHaveDefinedRenderingStates() {
        val line = prepare(source("x", 1000, 1000))
        assertTrue(RowRenderState(line.rows.single()).sweepCenter(1000).isFinite())
        val timeline =
            LyricsPlaybackTimeline(SyncedLyrics(emptyList()), PreparedLyrics(emptyList()))
        timeline.update(0)
        timeline.update(Int.MAX_VALUE)
        timeline.update(Int.MIN_VALUE)
        assertEquals(0, timeline.focus.value.firstIndex)
    }

    @Test
    fun sharedPreparedLyricsKeepIndependentClocksAndVisibility() {
        val accompaniment = KaraokeLine.AccompanimentKaraokeLine(
            listOf(KaraokeSyllable("echo", 500, 1500)), null,
            KaraokeAlignment.Start, 500, 1500,
        )
        val source = source("main", 1000, 3000).copy(accompanimentLines = listOf(accompaniment))
        val line = prepare(source)
        val prepared = PreparedLyrics(listOf(line))
        val lyrics = SyncedLyrics(listOf(source))
        val first = LyricsPlaybackTimeline(lyrics, prepared)
        val second = LyricsPlaybackTimeline(lyrics, prepared)
        val row = line.rows.single()
        val nested = line.before.single()

        first.update(1100)
        second.update(0)
        assertEquals(1100, first.state.row(row).time.intValue)
        assertEquals(Int.MIN_VALUE, second.state.row(row).time.intValue)
        assertTrue(first.state.line(nested).visible.value)
        assertFalse(second.state.line(nested).visible.value)

        second.update(4000)
        assertEquals(Int.MAX_VALUE, second.state.row(row).time.intValue)
        assertEquals(1100, first.state.row(row).time.intValue)
        assertTrue(first.state.line(nested).visible.value)
        first.update(0)
        assertEquals(Int.MAX_VALUE, second.state.row(row).time.intValue)
        assertFalse(first.state.line(nested).visible.value)
    }
}
