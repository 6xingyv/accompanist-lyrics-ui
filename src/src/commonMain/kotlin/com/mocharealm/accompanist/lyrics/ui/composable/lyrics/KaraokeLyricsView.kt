package com.mocharealm.accompanist.lyrics.ui.composable.lyrics

import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.unit.dp
import com.mocharealm.accompanist.lyrics.core.model.ISyncedLine
import com.mocharealm.accompanist.lyrics.core.model.SyncedLyrics
import kotlinx.coroutines.flow.Flow
import org.jetbrains.compose.resources.FontResource

/**
 * Native lyrics view backed by the Rust renderer.
 *
 * The public composable remains as the Compose entry point for integration with
 * existing screens, but all lyrics layout, hit testing, and drawing are delegated
 * to platform native hosts. Every visual/animation tunable lives in a single
 * [config] object (see [KaraokeLyricsConfig]); `KaraokeLyricsConfig()` reproduces
 * the previous look.
 *
 * When [backgroundArtwork] is supplied (Android), the same single surface also
 * paints a GPU mesh-gradient background derived from the artwork — turning this into
 * a full-bleed player background. [contentPadding] then insets the lyrics band below
 * the top bar / above the navigation bar, [isPlaying] gates the background's time
 * flow, and [backgroundReactive] enables loudness-driven reactivity (fed by
 * `NativeAudioAnalysis`). These are ignored on non-Android targets.
 *
 * Portrait player chrome: pass [playerChrome] to hand the complete three-page
 * player (lyrics / artwork / queue) to Rust. That path is mutually exclusive with
 * the legacy top bar driven by [title] / [artist] / [onControlsClick].
 *
 * Rust owns page switching and, when [useMusicFoundationClock] is enabled,
 * position / duration / play-pause via music-foundation's C ABI — the host must
 * not push those every frame. [onPlayerAction] still delivers transport, favorite,
 * more, media-output, and queue-filter actions for host-side control logic.
 */
@Suppress("UNUSED_PARAMETER")
@Composable
fun KaraokeLyricsView(
    lyrics: SyncedLyrics,
    currentPosition: () -> Int,
    positionUpdates: Flow<Int>? = null,
    onLineClicked: (ISyncedLine) -> Unit,
    onLinePressed: (ISyncedLine) -> Unit,
    modifier: Modifier = Modifier,
    config: KaraokeLyricsConfig = KaraokeLyricsConfig(),
    fontResource: FontResource? = null,
    backgroundArtwork: ImageBitmap? = null,
    contentPadding: PaddingValues = PaddingValues(0.dp),
    isPlaying: Boolean = true,
    isPlayingUpdates: Flow<Boolean>? = null,
    useMusicFoundationClock: Boolean = false,
    backgroundReactive: Boolean = false,
    title: String? = null,
    artist: String? = null,
    onControlsClick: (() -> Unit)? = null,
    playerChrome: NativePlayerChrome? = null,
    playerExpansionProgress: () -> Float = { 1f },
    onPlayerExpansionDragStart: (() -> Unit)? = null,
    onPlayerExpansionDrag: ((deltaY: Float) -> Unit)? = null,
    onPlayerExpansionDragEnd: ((velocityY: Float) -> Unit)? = null,
    onPlayerAction: ((NativePlayerAction) -> Unit)? = null,
    onQueueReordered: ((fromIndex: Int, toIndex: Int) -> Unit)? = null,
) {
    NativeLyricsViewHost(
        lyrics = lyrics,
        currentPosition = currentPosition,
        positionUpdates = positionUpdates,
        onLineClicked = onLineClicked,
        onLinePressed = onLinePressed,
        modifier = modifier,
        config = config,
        fontResource = fontResource,
        backgroundArtwork = backgroundArtwork,
        contentPadding = contentPadding,
        isPlaying = isPlaying,
        isPlayingUpdates = isPlayingUpdates,
        useMusicFoundationClock = useMusicFoundationClock,
        backgroundReactive = backgroundReactive,
        title = title,
        artist = artist,
        onControlsClick = onControlsClick,
        playerChrome = playerChrome,
        playerExpansionProgress = playerExpansionProgress,
        onPlayerExpansionDragStart = onPlayerExpansionDragStart,
        onPlayerExpansionDrag = onPlayerExpansionDrag,
        onPlayerExpansionDragEnd = onPlayerExpansionDragEnd,
        onPlayerAction = onPlayerAction,
        onQueueReordered = onQueueReordered,
    )
}
