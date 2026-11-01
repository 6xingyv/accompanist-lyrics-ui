package com.mocharealm.accompanist.lyrics.ui.internal.preparation

import com.mocharealm.accompanist.lyrics.core.model.karaoke.KaraokeAlignment
import com.mocharealm.accompanist.lyrics.core.model.karaoke.KaraokeLine
import com.mocharealm.accompanist.lyrics.core.model.karaoke.KaraokeSyllable
import com.mocharealm.accompanist.lyrics.ui.internal.layout.settledHeight
import com.mocharealm.accompanist.lyrics.ui.internal.playback.LyricsPlaybackState
import com.mocharealm.accompanist.lyrics.ui.internal.test.prepare
import com.mocharealm.accompanist.lyrics.ui.preparation.PreparedLine
import com.mocharealm.accompanist.lyrics.ui.preparation.PreparedLyrics
import kotlin.test.*

class VariantGeometryTest {
    /** Long romanization over single CJK characters reserves more slot width than the text needs. */
    private fun romajiLine(alignment: KaraokeAlignment = KaraokeAlignment.Start) =
        KaraokeLine.MainKaraokeLine(
            listOf(
                KaraokeSyllable("生 ", 1000, 2000, phonetic = "seikatsu"),
                KaraokeSyllable("道", 2000, 3000, phonetic = "douro"),
            ),
            null,
            alignment,
            1000,
            3000,
        )

    private fun resolvedWidths(): Pair<Float, Float> {
        val probe = prepare(romajiLine(), width = 1000f)
        val aligned = probe.runs.flatMap { it.groups }.sumOf { it.width.toDouble() }.toFloat()
        val compact =
            assertNotNull(probe.compact).runs.flatMap { it.groups }.sumOf { it.width.toDouble() }
                .toFloat()
        return aligned to compact
    }

    /** Fits the compact slots exactly, so only the reserved widths force a second row. */
    private fun squeezedLine(): PreparedLine {
        val (aligned, compact) = resolvedWidths()
        assertTrue(aligned > compact, "Fixture must reserve caption width")
        return prepare(romajiLine(), width = compact + 0.5f)
    }

    @Test
    fun reservedCaptionWidthProducesCompactGeometry() {
        val line = prepare(romajiLine(), width = 1000f)
        val compact =
            assertNotNull(line.compact, "A caption wider than its word must produce a variant")
        assertSame(line, line.variant(true))
        assertSame(compact, line.variant(false))
        assertTrue(line.rows.all { it.phoneticHeight > 0f }, "Aligned rows reserve caption space")
        assertTrue(compact.rows.all { it.phoneticHeight == 0f })
        assertTrue(compact.rows.all { it.phoneticSpacingBefore == 0f })
        for (group in compact.runs.flatMap { it.groups })
            assertEquals(group.textWidth, group.width, 0.01f, "Compact slots are text-tight")
    }

    @Test
    fun compactGeometryReusesShaping() {
        val line = prepare(romajiLine(), width = 1000f)
        val compact = assertNotNull(line.compact)
        val alignedUnits = line.runs.flatMap { it.groups }.flatMap { it.units }
        val compactUnits = compact.runs.flatMap { it.groups }.flatMap { it.units }
        assertEquals(alignedUnits.size, compactUnits.size)
        for (index in alignedUnits.indices) {
            assertSame(alignedUnits[index].text, compactUnits[index].text)
            assertSame(alignedUnits[index].text.layout, compactUnits[index].text.layout)
            assertNull(compactUnits[index].phonetic)
        }
    }

    @Test
    fun compactGeometryRewrapsWhenReservedWidthDisappears() {
        val line = squeezedLine()
        assertEquals(1, line.compact?.rows?.size, "Text-tight slots fit on one row")
        assertTrue(line.rows.size > 1, "Reserved slots no longer fit")
    }

    @Test
    fun bindingSlidesUnitsThatKeepTheirRow() {
        val binding = assertNotNull(squeezedLine().binding)
        assertEquals(2, binding.unitCount)
        assertTrue(binding.slides(0), "The first word keeps its row")
        assertFalse(binding.slides(1), "The second word moves up to the first row")
    }

    @Test
    fun endAlignedRowSlidesByTheFreedCaptionWidth() {
        // One row in both variants, so every unit slides instead of fading.
        val line = prepare(romajiLine(KaraokeAlignment.End), width = 1000f)
        val binding = assertNotNull(line.binding)
        assertTrue(binding.slides(0) && binding.slides(1))
        assertEquals(0f, binding.slideOffset(0, 1f), 0.001f, "Aligned geometry needs no offset")
        val freed = line.rows.single().width - assertNotNull(line.compact).rows.single().width
        assertTrue(freed > 0f)
        assertEquals(freed, binding.slideOffset(0, 0f), 0.001f)
    }

    @Test
    fun withoutReservedWidthThereIsNoSecondGeometry() {
        val line = prepare(
            KaraokeLine.MainKaraokeLine(
                listOf(KaraokeSyllable("生活", 1000, 3000, phonetic = "rei")),
                null,
                KaraokeAlignment.Start,
                1000,
                3000,
            ),
            width = 268f,
        )
        assertNull(line.compact, "A caption that fits inside its word moves nothing")
        assertSame(line, line.variant(false))
        assertNull(line.binding)
    }

    @Test
    fun settledHeightFollowsTheGeometryInUse() {
        val line = squeezedLine()
        val compact = assertNotNull(line.compact)
        val playback = LyricsPlaybackState(PreparedLyrics(listOf(line)))
        assertEquals(line.height, line.settledHeight(playback, false, true, 1f), 0.01f)
        assertEquals(compact.height, line.settledHeight(playback, false, false, 1f), 0.01f)
        assertTrue(compact.height < line.height, "Hidden captions must give their space back")
    }

    @Test
    fun bothGeometriesStayRegisteredForPlayback() {
        val line = squeezedLine()
        val compact = assertNotNull(line.compact)
        val lyrics = PreparedLyrics(listOf(line))
        assertTrue(line in lyrics.allLines)
        assertTrue(compact in lyrics.allLines)
        val playback = LyricsPlaybackState(lyrics)
        // Both row sets must resolve, otherwise drawing the hidden variant would throw.
        for (row in line.rows + compact.rows) playback.row(row)
    }
}
