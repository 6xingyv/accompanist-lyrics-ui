package com.mocharealm.accompanist.lyrics.ui.composable.lyrics

import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalLayoutDirection
import androidx.compose.ui.viewinterop.AndroidView
import com.mocharealm.accompanist.lyrics.core.model.ISyncedLine
import com.mocharealm.accompanist.lyrics.core.model.SyncedLyrics
import com.mocharealm.accompanist.lyrics.ui.renderer.RustSkiaLyricsView
import com.mocharealm.accompanist.lyrics.ui.renderer.toSceneStyle
import org.jetbrains.compose.resources.FontResource
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.launch

@Composable
internal actual fun NativeLyricsViewHost(
    lyrics: SyncedLyrics,
    currentPosition: () -> Int,
    positionUpdates: Flow<Int>?,
    onLineClicked: (ISyncedLine) -> Unit,
    onLinePressed: (ISyncedLine) -> Unit,
    modifier: Modifier,
    config: KaraokeLyricsConfig,
    fontResource: FontResource?,
    backgroundArtwork: ImageBitmap?,
    contentPadding: PaddingValues,
    isPlaying: Boolean,
    isPlayingUpdates: Flow<Boolean>?,
    useMusicFoundationClock: Boolean,
    backgroundReactive: Boolean,
    title: String?,
    artist: String?,
    onControlsClick: (() -> Unit)?,
) {
    val context = LocalContext.current
    val nativeView = remember(context) { NativeLyricsViewPool.acquire(context) }
    val scope = rememberCoroutineScope()
    val density = LocalDensity.current
    val fontResourceBytes = rememberFontResourceBytes(fontResource)
    val style = remember(config, density.density, density.fontScale) {
        config.toSceneStyle(density)
    }
    // Clef normally prepares this copy while loading the cover on Dispatchers.IO.
    // Keep a fallback for other hosts that do not opt into explicit prewarming.
    val backgroundArt = remember(backgroundArtwork) {
        backgroundArtwork?.let(NativeLyricsArtworkPrewarmer::getOrPrepare)
    }
    val layoutDirection = LocalLayoutDirection.current
    val contentTopPx = with(density) { contentPadding.calculateTopPadding().toPx() }
    val contentBottomPx = with(density) { contentPadding.calculateBottomPadding().toPx() }
    val contentLeftPx = with(density) { contentPadding.calculateLeftPadding(layoutDirection).toPx() }
    val contentRightPx =
        with(density) { contentPadding.calculateRightPadding(layoutDirection).toPx() }
    val latestControlsClick by rememberUpdatedState(onControlsClick)
    val latestLyrics by rememberUpdatedState(lyrics)
    val latestLineClicked by rememberUpdatedState(onLineClicked)
    val latestLinePressed by rememberUpdatedState(onLinePressed)
    val nativeControlsClick = remember {
        {
            latestControlsClick?.invoke()
            Unit
        }
    }
    val nativeLineClicked = remember {
        { index: Int ->
            latestLyrics.lines.getOrNull(index)?.let { latestLineClicked(it) }
            Unit
        }
    }
    val nativeLinePressed = remember {
        { index: Int ->
            latestLyrics.lines.getOrNull(index)?.let { latestLinePressed(it) }
            Unit
        }
    }

    // Player position samples arrive independently of Compose. This keeps the
    // native clock fresh for seeks and drift correction without re-running the
    // AndroidView update block (and its configuration checks) every poll.
    DisposableEffect(nativeView, positionUpdates) {
        val positionJob = positionUpdates?.let { updates ->
            scope.launch {
                updates.collect { nativeView.setCurrentPosition(it) }
            }
        }
        onDispose { positionJob?.cancel() }
    }
    DisposableEffect(nativeView, isPlayingUpdates, backgroundReactive) {
        val playbackJob = isPlayingUpdates?.let { updates ->
            scope.launch {
                updates.collect { playing ->
                    nativeView.setPlaybackState(playing, backgroundReactive)
                }
            }
        }
        onDispose { playbackJob?.cancel() }
    }

    fun RustSkiaLyricsView.applyAll() {
        // Background art + insets + playback FIRST: a fresh view/engine picks them
        // up before the scene is built, and re-applies survive font reconfig.
        setBackgroundArt(backgroundArt?.pixels, backgroundArt?.width ?: 0, backgroundArt?.height ?: 0)
        setContentInsets(contentTopPx, contentBottomPx, contentLeftPx, contentRightPx)
        setMusicFoundationClockEnabled(useMusicFoundationClock)
        setPlaybackState(isPlaying, backgroundReactive)
        setTopBar(title, artist)
        setOnControlsClicked(nativeControlsClick)
        configureFonts(fontResourceBytes)
        setStyle(style)
        setLyrics(lyrics)
        setCurrentPosition(currentPosition())
        setLineInteractionCallbacks(nativeLineClicked, nativeLinePressed)
    }

    AndroidView(
        factory = { context ->
            nativeView.apply {
                applyStateUpdate { applyAll() }
            }
        },
        update = { view -> view.applyStateUpdate { applyAll() } },
        onRelease = NativeLyricsViewPool::recycle,
        modifier = modifier.fillMaxSize()
    )
}
