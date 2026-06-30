package com.mocharealm.accompanist.lyrics.ui.composable.lyrics

import androidx.compose.runtime.Composable
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontListFontFamily
import com.mocharealm.accompanist.lyrics.text.NativeFontSource
import java.io.File

@Composable
actual fun getPlatformContext(): Any? {
    return null  // JVM doesn't need context
}

/**
 * Get font bytes from FontFamily on JVM/Desktop.
 * Supports:
 * - Resource fonts from classpath
 * - File-based fonts
 * - System fonts as fallback
 */
actual fun getFontSource(fontFamily: FontFamily?, platformContext: Any?): NativeFontSource? {
    // Try to extract font from FontFamily
    if (fontFamily is FontListFontFamily) {
        val fonts = fontFamily.fonts
        if (fonts.isNotEmpty()) {
            val font = fonts.first()
            
            // Try to get resource path via reflection
            try {
                val pathField = font.javaClass.getDeclaredField("resource")
                pathField.isAccessible = true
                val resourcePath = pathField.get(font) as? String
                if (resourcePath != null) {
                    val stream = Thread.currentThread().contextClassLoader?.getResourceAsStream(resourcePath)
                        ?: font.javaClass.getResourceAsStream(resourcePath)
                    if (stream != null) {
                        return NativeFontSource(bytes = stream.use { it.readBytes() })
                    }
                }
            } catch (e: Exception) {
                // Not a resource font
            }
            
            // Try to get file path
            try {
                val fileField = font.javaClass.getDeclaredField("file")
                fileField.isAccessible = true
                val file = fileField.get(font) as? File
                if (file != null && file.exists()) {
                    return NativeFontSource(path = file.absolutePath)
                }
            } catch (e: Exception) {
                // Not a file font
            }
        }
    }
    
    // Fallback to system fonts
    return getSystemFontSource()
}

private fun getSystemFontSource(): NativeFontSource? {
    val fontPaths = when {
        System.getProperty("os.name").lowercase().contains("win") -> listOf(
            "C:/Windows/Fonts/arial.ttf",
            "C:/Windows/Fonts/segoeui.ttf",
            "C:/Windows/Fonts/msyh.ttc",
            "C:/Windows/Fonts/simsun.ttc"
        )
        System.getProperty("os.name").lowercase().contains("mac") -> listOf(
            "/System/Library/Fonts/Helvetica.ttc",
            "/System/Library/Fonts/SFNS.ttf",
            "/Library/Fonts/Arial.ttf"
        )
        else -> listOf(
            "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
            "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
            "/usr/share/fonts/noto/NotoSans-Regular.ttf"
        )
    }
    
    for (path in fontPaths) {
        val file = File(path)
        if (file.exists() && file.canRead()) {
            return NativeFontSource(path = file.absolutePath)
        }
    }
    
    return null
}

/**
 * Get system fallback fonts for missing glyphs on JVM/Desktop.
 * Returns fonts in priority order, prioritizing CJK and wide Unicode coverage.
 */
actual fun getSystemFallbackFontSources(platformContext: Any?): List<NativeFontSource> {
    val result = mutableListOf<NativeFontSource>()

    val osName = System.getProperty("os.name").lowercase()
    
    val fallbackPaths = when {
        osName.contains("win") -> listOf(
            "C:/Windows/Fonts/msyh.ttc",      // Microsoft YaHei - Chinese
            "C:/Windows/Fonts/simsun.ttc",    // SimSun - Chinese
            "C:/Windows/Fonts/meiryo.ttc",    // Meiryo - Japanese
            "C:/Windows/Fonts/malgun.ttf",    // Malgun Gothic - Korean
            "C:/Windows/Fonts/arial.ttf",     // Arial - Latin
            "C:/Windows/Fonts/seguisym.ttf"   // Segoe UI Symbol - Emoji/symbols
        )
        osName.contains("mac") -> listOf(
            "/System/Library/Fonts/PingFang.ttc",         // Chinese
            "/System/Library/Fonts/Hiragino Sans GB.ttc", // Chinese
            "/System/Library/Fonts/Hiragino.ttc",         // Japanese
            "/System/Library/Fonts/AppleGothic.ttf",      // Korean
            "/System/Library/Fonts/Helvetica.ttc",        // Latin
            "/System/Library/Fonts/Apple Color Emoji.ttc" // Emoji
        )
        else -> listOf(
            "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/google-noto-cjk/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
            "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf"
        )
    }
    
    for (path in fallbackPaths) {
        val file = File(path)
        if (file.exists() && file.canRead()) {
            result.add(NativeFontSource(path = file.absolutePath))
        }
    }

    return result
}
