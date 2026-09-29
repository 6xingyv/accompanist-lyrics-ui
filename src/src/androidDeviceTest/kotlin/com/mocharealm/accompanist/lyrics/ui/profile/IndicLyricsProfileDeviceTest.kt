package com.mocharealm.accompanist.lyrics.ui.profile

import android.content.Context
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Canvas
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.Shadow
import androidx.compose.ui.graphics.drawscope.CanvasDrawScope
import androidx.compose.ui.graphics.drawscope.clipRect
import androidx.compose.ui.graphics.toPixelMap
import androidx.compose.ui.text.TextMeasurer
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.drawText
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.font.createFontFamilyResolver
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.ui.unit.sp
import androidx.test.platform.app.InstrumentationRegistry
import com.mocharealm.accompanist.lyrics.core.model.karaoke.KaraokeSyllable
import kotlin.test.assertEquals
import kotlin.test.assertTrue
import org.junit.Test

class IndicLyricsProfileDeviceTest {
    @Test
    fun wholeIndicWordSurvivesUnderestimatedBoundsOnAndroidCanvas() {
        val context: Context = InstrumentationRegistry.getInstrumentation().targetContext
        val density = Density(context.resources.displayMetrics.density)
        val measurer = TextMeasurer(createFontFamilyResolver(context), density, LayoutDirection.Ltr)
        val text = "यूँ"
        val style = TextStyle(fontSize = 34.sp, fontWeight = FontWeight.Bold)
        val measured = IndicProfile.prepare(listOf(KaraokeSyllable(text, 0, 100)), measurer, style).single()
        val unit = measured.copy(right = measured.left + 1f)
        assertTrue(unit.width > 0f)

        fun render(draw: androidx.compose.ui.graphics.drawscope.DrawScope.() -> Unit): ImageBitmap {
            val bitmap = ImageBitmap(256, 128)
            CanvasDrawScope().draw(density, LayoutDirection.Ltr, Canvas(bitmap), Size(256f, 128f)) {
                drawRect(Color.White)
                draw()
            }
            return bitmap
        }

        val expected = render { drawText(unit.layout, Color.Black) }
        val actual = render { with(IndicProfile) { draw(unit, Color.Black, Shadow.None) } }
        val oldClip = render {
            clipRect(
                0f,
                -unit.height,
                if (unit.right >= unit.layout.getLineRight(0)) unit.width + unit.height else unit.width,
                unit.layout.size.height * 2f,
            ) {
                drawText(unit.layout, Color.Black, topLeft = Offset(-unit.left, 0f))
            }
        }

        val expectedPixels = expected.toPixelMap()
        val actualPixels = actual.toPixelMap()
        val oldClipPixels = oldClip.toPixelMap()
        var inkPixels = 0
        var oldClipDifference = 0
        for (y in 0 until expected.height) for (x in 0 until expected.width) {
            assertEquals(expectedPixels[x, y], actualPixels[x, y], "new renderer pixel ($x, $y)")
            if (expectedPixels[x, y] != Color.White) inkPixels++
            if (expectedPixels[x, y] != oldClipPixels[x, y]) oldClipDifference++
        }
        assertTrue(inkPixels > 0, "Android must render the Hindi sample")
        assertTrue(oldClipDifference > 0, "The underestimated old clip should remove visible glyph pixels")
    }
}
