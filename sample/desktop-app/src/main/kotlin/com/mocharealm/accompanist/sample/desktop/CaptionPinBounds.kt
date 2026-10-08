package com.mocharealm.accompanist.sample.desktop

internal const val CAPTION_HEIGHT = 36f

/** Shared by Compose placement and JBR hit testing; dimensions are logical dp. */
internal data class CaptionPinBounds(val left: Float, val top: Float, val width: Float, val height: Float) {
    val right get() = left + width
    fun contains(x: Float, y: Float): Boolean =
        x >= left && x < right && y >= top && y < top + height
}

internal fun captionPinBounds(
    platform: DesktopPlatform,
    windowWidth: Float,
    leftInset: Float,
    rightInset: Float,
): CaptionPinBounds = when (platform) {
    DesktopPlatform.Windows -> {
        // Each native minimize/maximize/close button occupies one third of the inset.
        val width = if (rightInset > 0f) rightInset / 3f else 46f
        CaptionPinBounds(windowWidth - rightInset - width, 0f, width, CAPTION_HEIGHT)
    }
    DesktopPlatform.Mac -> CaptionPinBounds(leftInset + 6f, 4f, 28f, 28f)
    DesktopPlatform.Linux -> CaptionPinBounds(windowWidth - rightInset - 44f, 0f, 36f, CAPTION_HEIGHT)
}
