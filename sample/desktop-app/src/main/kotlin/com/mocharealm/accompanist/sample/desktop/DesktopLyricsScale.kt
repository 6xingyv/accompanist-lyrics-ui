package com.mocharealm.accompanist.sample.desktop

/** Port of feature-text-engine's renderer/layout.rs, in logical window-content dp. */
internal fun desktopLyricsScale(width: Float, height: Float): Float {
    if (width < 600f || width / height.coerceAtLeast(1f) < 1.3f) return 1f
    val widthProgress = ((width - 1024f) / (1600f - 1024f)).coerceIn(0f, 1f)
    val heightProgress = ((height - 512f) / (1000f - 512f)).coerceIn(0f, 1f)
    return 1f + minOf(widthProgress, heightProgress) * 0.4f
}
