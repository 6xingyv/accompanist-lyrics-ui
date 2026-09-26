package com.mocharealm.accompanist.lyrics.ui.internal.rendering

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

class LyricsRenderingTest {
    @Test
    fun sweepUsesTwoEmAndCoversExactLogicalEndpoints() {
        for (text in listOf("Hello", "生活", "سلام")) {
            val line = prepare(source(text, 1000, 4000), 100f)
            for (row in line.rows) {
                assertEquals(style.fontSize.value * 2f, row.sweepFadeWidth, 0.001f)
                val half = row.sweepFadeWidth / 2f
                val start = RowRenderState(row).sweepCenter(row.sweepStarts.first())
                val end = RowRenderState(row).sweepCenter(row.sweepEnds.last())
                if (row.rtl) {
                    assertEquals(row.sweepRight.first(), start - half, 0.001f)
                    assertEquals(row.sweepLeft.last(), end + half, 0.001f)
                } else {
                    assertEquals(row.sweepLeft.first(), start + half, 0.001f)
                    assertEquals(row.sweepRight.last(), end - half, 0.001f)
                }
            }
        }
    }

    @Test
    fun rowsWithoutEffectsHaveNoLayerStorageRequirements() {
        val line = prepare(source("hello", 0, 300))
        val raster = prepareLineRaster(line, Color.White, Density(1f), LayoutDirection.Ltr)
        for (row in raster.rows) {
            assertFalse(row.hasGlow)
            assertFalse(row.hasPhonetics)
        }
    }

    @Test
    fun singleDrawableGroupsShareTheirCombinedRaster() {
        val line = prepare(source("我的生活", 0, 10000))
        val raster = prepareLineRaster(line, Color.White, Density(1f), LayoutDirection.Ltr)
        for (row in raster.rows) for (run in row.runs) for (group in run) {
            assertEquals(1, group.units.size)
            assertSame(group.combined, group.units.single().text)
        }
    }

    @Test
    fun rasterCacheSurvivesItemsAndInvalidatesForColor() {
        val line = prepare(source("Singing", 0, 10000))
        val prepared = PreparedLyrics(listOf(line))
        val resources = LyricsRenderResources(prepared, Color.White, Density(1f), LayoutDirection.Ltr)
        val raster = resources.raster(line)
        assertSame(raster, resources.raster(line))
        val red = LyricsRenderResources(prepared, Color.Red, Density(1f), LayoutDirection.Ltr)
        assertNotSame(raster, red.raster(line))
        assertSame(raster, resources.raster(line), "Another host cannot replace this host's raster")
        val layers = raster.rows.single()
        val units = layers.runs.single().single().units
        assertTrue(units.size > 1)
        assertEquals(1, layers.pageCount)
        for (unit in units) {
            assertTrue(unit.glow)
            assertSame(units.first().text.image, unit.text.image)
        }
        val paints = RowPaints(Color.White)
        assertNotEquals(paints.blurEffects[1], paints.blurEffects[64])
    }

    @Test
    fun rowMaskRendersInactiveActiveAndFinishedWithoutPreparing() {
        val line = prepare(source("Ahhh This is 我的生活 不安です"), 500f)
        val images =
            listOf(0, 1800, 5000).map { time ->
                val bitmap = ImageBitmap(600, 200)
                val canvas = Canvas(bitmap)
                CanvasDrawScope().draw(Density(1f), LayoutDirection.Ltr, canvas, Size(600f, 200f)) {
                    for (row in line.rows) {
                        drawPreparedRow(row, time, RowRenderState(row), Color.White, RowPaints(Color.White), false)
                    }
                }
                bitmap
            }
        fun alphaSum(bitmap: ImageBitmap): Double {
            val pixels = bitmap.toPixelMap()
            var sum = 0.0
            for (y in 0 until bitmap.height) for (x in 0 until bitmap.width) sum +=
                pixels[x, y].alpha
            return sum
        }
        assertTrue(alphaSum(images[0]) > 0)
        assertTrue(alphaSum(images[2]) > alphaSum(images[0]) * 3)
        assertTrue(alphaSum(images[1]) > alphaSum(images[0]))
        assertTrue(alphaSum(images[1]) < alphaSum(images[2]))
        val output = java.io.File("build/reports/rendering").apply { mkdirs() }
        images.forEachIndexed { i, bitmap ->
            java.io
                .File(output, "row-$i.png")
                .writeBytes(
                    bitmap.asSkiaBitmap().let {
                        org.jetbrains.skia.Image.makeFromBitmap(it).encodeToData()!!.bytes
                    }
                )
        }
    }
}
