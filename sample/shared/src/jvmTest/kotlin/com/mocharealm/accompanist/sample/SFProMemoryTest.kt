package com.mocharealm.accompanist.sample

import androidx.compose.ui.text.font.FontListFontFamily
import androidx.compose.ui.text.font.FontVariation
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.platform.FileFont
import androidx.compose.ui.text.platform.Font
import androidx.compose.ui.text.TextMeasurer
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.createFontFamilyResolver
import androidx.compose.ui.text.drawText
import androidx.compose.ui.graphics.*
import androidx.compose.ui.graphics.drawscope.CanvasDrawScope
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.ui.unit.sp
import com.mocharealm.accompanist.sample.ui.theme.sfProFamily
import java.io.File
import java.nio.file.Files
import kotlin.test.*

class SFProMemoryTest {
    @Test fun fontWeightsShareOneFileAndKeepVariableWeightSettings() {
        val file = Files.createTempFile("sf-pro-test-", ".ttf").toFile().apply { deleteOnExit() }
        val family = sfProFamily(file) as FontListFontFamily
        assertEquals(listOf(FontWeight.ExtraLight, FontWeight.Light, FontWeight.Normal,
            FontWeight.Medium, FontWeight.SemiBold, FontWeight.Bold, FontWeight.ExtraBold),
            family.fonts.map { it.weight })
        for (font in family.fonts) {
            val loaded = font as FileFont
            assertSame(file, loaded.file, "Each weight must reuse the same backing file")
            assertEquals(FontVariation.Settings(FontVariation.weight(font.weight.weight)).settings,
                loaded.variationSettings.settings)
        }
    }

    @Test fun fileBackedWeightsKeepTheOriginalGlyphsAndMeasurements() {
        val file = File("src/commonMain/composeResources/font/sf_pro.ttf")
        val bytes = file.readBytes()
        val family = sfProFamily(file)
        val measurer = TextMeasurer(createFontFamilyResolver(), Density(1f), LayoutDirection.Ltr)
        for (weight in listOf(FontWeight.Normal, FontWeight.SemiBold, FontWeight.Bold)) {
            val variations = FontVariation.Settings(FontVariation.weight(weight.weight))
            val original = FontFamily(Font("sf-pro-memory-reference-${weight.weight}", bytes,
                weight = weight, variationSettings = variations))
            val style = TextStyle(fontSize = 32.sp, fontWeight = weight)
            val expected = measurer.measure("Morning light 123 中文", style.copy(fontFamily = original))
            val actual = measurer.measure("Morning light 123 中文", style.copy(fontFamily = family))
            assertEquals(expected.size, actual.size, "Font geometry changed for $weight")
            assertEquals(expected.firstBaseline, actual.firstBaseline)
            fun pixels(layout: androidx.compose.ui.text.TextLayoutResult): IntArray {
                val bitmap = ImageBitmap(layout.size.width + 24, layout.size.height + 24)
                CanvasDrawScope().draw(Density(1f), LayoutDirection.Ltr, Canvas(bitmap),
                    Size(bitmap.width.toFloat(), bitmap.height.toFloat())) {
                    drawText(layout, Color.White)
                }
                return IntArray(bitmap.width * bitmap.height).also { bitmap.readPixels(it) }
            }
            assertContentEquals(pixels(expected), pixels(actual), "Font appearance changed for $weight")
        }
    }
}
