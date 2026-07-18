package com.mocharealm.accompanist.lyrics.ui.composable.lyrics

import android.content.Context
import android.graphics.BitmapFactory
import android.net.Uri
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalLayoutDirection
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import com.mocharealm.accompanist.lyrics.core.model.ISyncedLine
import com.mocharealm.accompanist.lyrics.core.model.SyncedLyrics
import com.mocharealm.accompanist.lyrics.ui.renderer.RustSkiaLyricsView
import com.mocharealm.accompanist.lyrics.ui.renderer.toSceneStyle
import org.jetbrains.compose.resources.FontResource
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

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
    playerChrome: NativePlayerChrome?,
    playerExpansionProgress: Float,
    onPlayerAction: ((NativePlayerAction) -> Unit)?,
    onQueueReordered: ((Int, Int) -> Unit)?,
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
    val latestPlayerAction by rememberUpdatedState(onPlayerAction)
    val latestQueueReordered by rememberUpdatedState(onQueueReordered)
    val latestLyrics by rememberUpdatedState(lyrics)
    val latestLineClicked by rememberUpdatedState(onLineClicked)
    val latestLinePressed by rememberUpdatedState(onLinePressed)
    val nativeControlsClick = remember {
        {
            latestControlsClick?.invoke()
            Unit
        }
    }
    val nativePlayerAction = remember {
        { code: Int ->
            NativePlayerAction.fromCode(code)?.let { latestPlayerAction?.invoke(it) }
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
    val nativeQueueReordered = remember {
        { from: Int, to: Int ->
            latestQueueReordered?.invoke(from, to)
            Unit
        }
    }
    val queueArtworkUris = remember(playerChrome?.queueItems) {
        playerChrome?.queueItems
            ?.mapNotNull { it.artworkUri?.takeIf(String::isNotBlank) }
            ?.distinct()
            .orEmpty()
    }
    val queueArtworkTargetPx = with(density) { 96.dp.roundToPx() }.coerceAtLeast(48)

    LaunchedEffect(nativeView, queueArtworkUris, queueArtworkTargetPx) {
        nativeView.clearQueueArtworks()
        if (queueArtworkUris.isEmpty()) return@LaunchedEffect
        queueArtworkUris.forEach { uri ->
            val decoded = withContext(Dispatchers.IO) {
                decodeQueueArtwork(context, uri, queueArtworkTargetPx)
            } ?: return@forEach
            nativeView.setQueueArtwork(uri, decoded.first, decoded.second, decoded.third)
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
        setPlayerExpansionProgress(playerExpansionProgress)
        if (playerChrome != null) {
            // Full portrait player owns chrome geometry; clear the legacy top bar.
            // Screen/duration/playing: Rust keeps page selection after first paint
            // and overrides transport from music-foundation when enabled. Wire
            // values only seed fallbacks / first appearance.
            setTopBar(null, null)
            setOnControlsClicked(null)
            setPlayerChrome(
                title = playerChrome.title,
                artist = playerChrome.artist,
                durationMs = playerChrome.durationMs,
                playing = playerChrome.isPlaying,
                liked = playerChrome.liked,
                presentation = playerChrome.presentation.wireValue,
                screen = playerChrome.initialScreen.wireValue,
                queueTitle = playerChrome.queueTitle,
                queueSource = playerChrome.queueSource,
                queueFilter = playerChrome.queueFilter.wireValue,
                queueItems = playerChrome.queueItems.map {
                    Triple(it.title, it.artist, it.artworkUri)
                },
            )
            setOnPlayerAction(nativePlayerAction)
            setOnQueueReordered(nativeQueueReordered)
        } else {
            setPlayerChrome(title = null)
            setOnPlayerAction(null)
            setOnQueueReordered(null)
            setTopBar(title, artist)
            setOnControlsClicked(nativeControlsClick)
        }
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

private fun decodeQueueArtwork(
    context: Context,
    uriString: String,
    targetPx: Int,
): Triple<IntArray, Int, Int>? = runCatching {
    val uri = Uri.parse(uriString)
    val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
    context.contentResolver.openInputStream(uri)?.use { BitmapFactory.decodeStream(it, null, bounds) }
    if (bounds.outWidth <= 0 || bounds.outHeight <= 0) return@runCatching null
    var sample = 1
    while (bounds.outWidth / (sample * 2) >= targetPx &&
        bounds.outHeight / (sample * 2) >= targetPx
    ) {
        sample *= 2
    }
    val options = BitmapFactory.Options().apply {
        inSampleSize = sample
        inPreferredConfig = android.graphics.Bitmap.Config.ARGB_8888
    }
    val bitmap = context.contentResolver.openInputStream(uri)?.use {
        BitmapFactory.decodeStream(it, null, options)
    } ?: return@runCatching null
    try {
        val pixels = IntArray(bitmap.width * bitmap.height)
        bitmap.getPixels(pixels, 0, bitmap.width, 0, 0, bitmap.width, bitmap.height)
        Triple(pixels, bitmap.width, bitmap.height)
    } finally {
        bitmap.recycle()
    }
}.getOrNull()
