package com.mocharealm.accompanist.lyrics.ui.internal.playback

import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.*
import androidx.compose.ui.graphics.drawscope.CanvasDrawScope
import androidx.compose.ui.text.TextMeasurer
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.createFontFamilyResolver
import androidx.compose.ui.unit.*
import com.mocharealm.accompanist.lyrics.core.model.SyncedLyrics
import com.mocharealm.accompanist.lyrics.core.model.synced.SyncedLine
import com.mocharealm.accompanist.lyrics.ui.internal.rendering.*
import com.mocharealm.accompanist.lyrics.ui.preparation.prepareLyrics
import com.mocharealm.accompanist.lyrics.ui.profile.DefaultLyricsProfiles
import kotlin.test.*

class SyncedHighlightTest {
    @Test fun staticLineActuallyBrightensAtStartAndDimsAgainAfterBackwardSeek() {
        val lyrics=SyncedLyrics(listOf(SyncedLine("Solid text",null,1000,3000)))
        val style=TextStyle(fontSize=32.sp)
        val prepared=prepareLyrics(lyrics,DefaultLyricsProfiles,
            TextMeasurer(createFontFamilyResolver(),Density(1f),LayoutDirection.Ltr),
            style,style,style,400f,1f,false,translationStyle=style)
        val row=prepared.lines.single()!!.rows.single()
        assertFalse(row.animated)
        val timeline=LyricsPlaybackTimeline(lyrics,prepared)
        val paint=RowPaints(Color.White)
        val state=RowRenderState(row)
        fun alpha():Int {
            val image=ImageBitmap(450,100)
            CanvasDrawScope().draw(Density(1f),LayoutDirection.Ltr,Canvas(image),Size(450f,100f)) {
                drawPreparedRow(row,timeline.state.row(row).time.intValue,state,Color.White,paint,false)
            }
            val pixels=IntArray(450*100);image.readPixels(pixels)
            return pixels.maxOf{it ushr 24}
        }
        timeline.update(900)
        assertEquals(51,alpha())
        // This is an ordinary clock tick, not the seek path that refreshes all rows.
        timeline.update(1000)
        assertEquals(255,alpha())
        assertFalse(timeline.state.row(row).isAnimating.value)
        timeline.update(1100)
        assertEquals(255,alpha())
        timeline.update(200)
        assertEquals(51,alpha())
        timeline.update(3500)
        assertEquals(255,alpha())
    }
}
