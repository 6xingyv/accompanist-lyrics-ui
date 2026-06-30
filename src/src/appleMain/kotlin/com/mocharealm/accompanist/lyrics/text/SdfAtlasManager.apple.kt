package com.mocharealm.accompanist.lyrics.text

import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Shadow
import androidx.compose.ui.graphics.drawscope.DrawScope

/**
 * Apple platform implementation of SdfAtlasManager.
 * Currently a stub implementation - native text engine is not yet fully implemented for Apple.
 */
actual class SdfAtlasManager actual constructor(
    atlasWidth: Int,
    atlasHeight: Int
) {
    actual val width: Int = atlasWidth
    actual val height: Int = atlasHeight
    
    // TODO: Implement using Metal/Core Graphics when Apple native text engine is ready
    
    actual fun updateAtlas(uploads: List<GlyphUpload>) {
        // Stub - not implemented for Apple yet
    }
    
    actual fun DrawScope.drawGlyph(
        atlasRect: Rect,
        destOffset: Offset,
        destSize: Size,
        color: Color,
        shadow: Shadow?
    ) {
        // Stub - draw placeholder rectangle for now
        drawRect(
            color = color,
            topLeft = destOffset,
            size = destSize
        )
    }
    
    actual fun isReady(): Boolean = false

    actual fun clear() {
        // No-op
    }
    
    actual fun destroy() {
        // No-op
    }
}
