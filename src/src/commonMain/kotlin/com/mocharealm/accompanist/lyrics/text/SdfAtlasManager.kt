package com.mocharealm.accompanist.lyrics.text

import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Shadow
import androidx.compose.ui.graphics.drawscope.DrawScope

/**
 * Represents a pending glyph upload from native engine.
 * Contains RGBA pixel data for a region of the atlas.
 */
data class GlyphUpload(
    val x: Int,
    val y: Int,
    val width: Int,
    val height: Int,
    val data: ByteArray
) {
    override fun equals(other: Any?): Boolean {
        if (this === other) return true
        if (other == null || this::class != other::class) return false
        other as GlyphUpload
        return x == other.x && y == other.y && width == other.width && height == other.height
    }

    override fun hashCode(): Int {
        var result = x
        result = 31 * result + y
        result = 31 * result + width
        result = 31 * result + height
        return result
    }
}

/**
 * Manages the SDF atlas texture for text rendering.
 * This is a platform-specific implementation that handles:
 * - Atlas texture creation and updates
 * - Drawing glyphs from the atlas using SDF techniques
 */
expect class SdfAtlasManager(atlasWidth: Int, atlasHeight: Int) {
    /**
     * The width of the atlas texture in pixels.
     */
    val width: Int
    
    /**
     * The height of the atlas texture in pixels.
     */
    val height: Int
    
    /**
     * Updates the atlas texture with pending uploads from the native engine.
     * Call this before drawing if the engine has pending uploads.
     *
     * @param uploads List of glyph regions to upload to the atlas
     */
    fun updateAtlas(uploads: List<GlyphUpload>)
    
    /**
     * Draws a glyph from the atlas to the canvas.
     * Uses SDF rendering for crisp text at any scale.
     *
     * @param scope The DrawScope to draw into
     * @param atlasRect The source rectangle in the atlas (x, y, width, height from NativeLayoutResult.atlas_rects)
     * @param destOffset The destination position on canvas
     * @param destSize The destination size (for scaling)
     * @param color The text color to render
     */
    fun DrawScope.drawGlyph(
        atlasRect: Rect,
        destOffset: Offset,
        destSize: Size,
        color: Color,
        shadow: Shadow? = null
    )
    
    /**
     * Checks if the atlas is ready for rendering.
     * @return true if the atlas has been initialized with texture data
     */
    fun isReady(): Boolean

    /**
     * Clears atlas pixels without destroying the manager.
     */
    fun clear()
    
    /**
     * Releases all resources associated with the atlas.
     */
    fun destroy()
}

/**
 * Parses the JSON string from NativeTextEngine.getPendingUploadsJson() into a list of GlyphUpload.
 * JSON format: [{"x":0,"y":0,"width":32,"height":32,"data":"base64..."},...]
 */
internal fun parsePendingUploads(json: String): List<GlyphUpload> {
    if (json.isEmpty() || json == "[]") return emptyList()
    
    val uploads = mutableListOf<GlyphUpload>()
    
    try {
        // Simple regex-based JSON array parsing
        // Match each object in the array
        val objectPattern = Regex("""\{[^}]+\}""")
        val matches = objectPattern.findAll(json)
        
        for (match in matches) {
            val obj = match.value
            
            val x = Regex(""""x"\s*:\s*(\d+)""").find(obj)?.groupValues?.get(1)?.toIntOrNull() ?: continue
            val y = Regex(""""y"\s*:\s*(\d+)""").find(obj)?.groupValues?.get(1)?.toIntOrNull() ?: continue
            val width = Regex(""""width"\s*:\s*(\d+)""").find(obj)?.groupValues?.get(1)?.toIntOrNull() ?: continue
            val height = Regex(""""height"\s*:\s*(\d+)""").find(obj)?.groupValues?.get(1)?.toIntOrNull() ?: continue
            val dataBase64 = Regex(""""data"\s*:\s*"([^"]+)"""").find(obj)?.groupValues?.get(1) ?: continue
            
            val data = decodeBase64(dataBase64)
            
            uploads.add(GlyphUpload(x, y, width, height, data))
        }
    } catch (e: Exception) {
        // Return empty list on parse error
    }
    
    return uploads
}

/**
 * Simple base64 decoder (no padding handling for simplicity, matching Rust encoder).
 */
private fun decodeBase64(input: String): ByteArray {
    val chars = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/"
    val output = mutableListOf<Byte>()
    
    var buffer = 0
    var bitsCollected = 0
    
    for (c in input) {
        if (c == '=') break
        val value = chars.indexOf(c)
        if (value < 0) continue
        
        buffer = (buffer shl 6) or value
        bitsCollected += 6
        
        if (bitsCollected >= 8) {
            bitsCollected -= 8
            output.add(((buffer shr bitsCollected) and 0xFF).toByte())
        }
    }
    
    return output.toByteArray()
}

/**
 * Composable to remember an SdfAtlasManager instance.
 */
@Composable
fun rememberSdfAtlasManager(atlasWidth: Int, atlasHeight: Int): SdfAtlasManager {
    return remember(atlasWidth, atlasHeight) {
        SdfAtlasManager(atlasWidth, atlasHeight)
    }
}

/**
 * Pre-caches printable ASCII characters (32-126) for faster rendering.
 * Should be called during app initialization or when fonts are loaded.
 * 
 * @param nativeEngine The NativeTextEngine to use for glyph generation
 * @param atlasManager The SdfAtlasManager to update with generated glyphs
 * @param fontSize The font size in pixels to cache at
 * @param fontWeight The font weight to use (default 400)
 */
suspend fun warmupAscii(
    nativeEngine: NativeTextEngine,
    atlasManager: SdfAtlasManager,
    fontSize: Float,
    fontWeight: Float = 400f
) {
    kotlinx.coroutines.withContext(kotlinx.coroutines.Dispatchers.Default) {
        // ASCII printable characters: space (32) to tilde (126)
        val asciiChars = (32..126).map { it.toChar() }.joinToString("")
        
        // Process all ASCII chars to generate glyphs
        nativeEngine.processText(asciiChars, fontSize, fontWeight)
        
        // Upload pending glyphs to atlas
        val uploads = nativeEngine.drainPendingUploads()
        if (uploads.isNotEmpty()) {
            kotlinx.coroutines.withContext(kotlinx.coroutines.Dispatchers.Main) {
                atlasManager.updateAtlas(uploads)
            }
        }
    }
}

