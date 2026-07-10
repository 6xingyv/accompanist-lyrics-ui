package com.mocharealm.accompanist.lyrics.ui.composable.lyrics

import android.graphics.Bitmap
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalLayoutDirection
import androidx.compose.ui.viewinterop.AndroidView
import com.mocharealm.accompanist.lyrics.core.model.ISyncedLine
import com.mocharealm.accompanist.lyrics.core.model.SyncedLyrics
import com.mocharealm.accompanist.lyrics.ui.renderer.RustSkiaLyricsView
import com.mocharealm.accompanist.lyrics.ui.renderer.toSceneStyle
import org.jetbrains.compose.resources.FontResource
import kotlin.math.max

/** Longest edge (px) of the artwork handed to the native mesh builder. It is
 * downscaled again to 32×32 inside the engine, so a small copy is plenty and keeps
 * the getPixels + JNI transfer cheap. */
private const val BACKGROUND_ART_MAX_EDGE = 160

private class BackgroundArt(val pixels: IntArray, val width: Int, val height: Int)

private fun ImageBitmap.toBackgroundArt(): BackgroundArt {
    val source = asAndroidBitmap()
    val longest = max(source.width, source.height)
    val scale = if (longest > BACKGROUND_ART_MAX_EDGE) {
        BACKGROUND_ART_MAX_EDGE.toFloat() / longest
    } else {
        1f
    }
    val w = max(1, (source.width * scale).toInt())
    val h = max(1, (source.height * scale).toInt())
    val scaled = if (scale < 1f) {
        Bitmap.createScaledBitmap(source, w, h, true)
    } else {
        source
    }
    val pixels = IntArray(scaled.width * scaled.height)
    scaled.getPixels(pixels, 0, scaled.width, 0, 0, scaled.width, scaled.height)
    return BackgroundArt(pixels, scaled.width, scaled.height)
}

@Composable
internal actual fun NativeLyricsViewHost(
    lyrics: SyncedLyrics,
    currentPosition: () -> Int,
    onLineClicked: (ISyncedLine) -> Unit,
    onLinePressed: (ISyncedLine) -> Unit,
    modifier: Modifier,
    config: KaraokeLyricsConfig,
    fontResource: FontResource?,
    backgroundArtwork: ImageBitmap?,
    contentPadding: PaddingValues,
    isPlaying: Boolean,
    backgroundReactive: Boolean,
    title: String?,
    artist: String?,
    onControlsClick: (() -> Unit)?,
) {
    val density = LocalDensity.current
    val fontResourceBytes = rememberFontResourceBytes(fontResource)
    val style = config.toSceneStyle(density)
    // Convert the artwork to a downscaled pixel copy once per bitmap.
    val backgroundArt = remember(backgroundArtwork) { backgroundArtwork?.toBackgroundArt() }
    val layoutDirection = LocalLayoutDirection.current
    val contentTopPx = with(density) { contentPadding.calculateTopPadding().toPx() }
    val contentBottomPx = with(density) { contentPadding.calculateBottomPadding().toPx() }
    val contentLeftPx = with(density) { contentPadding.calculateLeftPadding(layoutDirection).toPx() }
    val contentRightPx =
        with(density) { contentPadding.calculateRightPadding(layoutDirection).toPx() }
    val latestControlsClick by rememberUpdatedState(onControlsClick)

    fun RustSkiaLyricsView.applyAll() {
        // Background art + insets + playback FIRST: a fresh view/engine picks them
        // up before the scene is built, and re-applies survive font reconfig.
        setBackgroundArt(backgroundArt?.pixels, backgroundArt?.width ?: 0, backgroundArt?.height ?: 0)
        setContentInsets(contentTopPx, contentBottomPx, contentLeftPx, contentRightPx)
        setPlaybackState(isPlaying, backgroundReactive)
        setTopBar(title, artist)
        setOnControlsClicked { latestControlsClick?.invoke() }
        configureFonts(fontResourceBytes)
        setStyle(style)
        setLyrics(lyrics)
        setCurrentPosition(currentPosition())
        setLineInteractionCallbacks(
            onLineClicked = { index -> lyrics.lines.getOrNull(index)?.let(onLineClicked) },
            onLinePressed = { index -> lyrics.lines.getOrNull(index)?.let(onLinePressed) }
        )
    }

    AndroidView(
        factory = { context -> RustSkiaLyricsView(context).apply { applyAll() } },
        update = { view -> view.applyAll() },
        modifier = modifier.fillMaxSize()
    )
}
