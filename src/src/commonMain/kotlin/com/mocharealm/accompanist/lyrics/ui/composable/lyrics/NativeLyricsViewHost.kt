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

@Composable
internal expect fun NativeLyricsViewHost(
    lyrics: SyncedLyrics,
    currentPosition: () -> Int,
    positionUpdates: Flow<Int>? = null,
    onLineClicked: (ISyncedLine) -> Unit,
    onLinePressed: (ISyncedLine) -> Unit,
    modifier: Modifier,
    config: KaraokeLyricsConfig,
    fontResource: FontResource?,
    // Full-bleed GPU mesh-gradient background (Android). `backgroundArtwork` enables
    // it; `contentPadding` carries the SYSTEM insets (status/caption top, nav bottom);
    // `isPlaying`/`backgroundReactive` drive the animation. When `playerChrome` is
    // non-null, Rust owns the complete three-page portrait player and
    // `onPlayerAction` receives button codes. Otherwise a non-null `title` enables
    // the legacy top bar (thumbnail + title/artist + ⋯) and `onControlsClick` fires
    // on a ⋯ tap. All ignored off Android.
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
    playerExpansionTarget: Float = 1f,
    playerExpansionGeometry: NativePlayerExpansionGeometry? = null,
    onPlayerExpansionDragStart: (() -> Unit)? = null,
    onPlayerExpansionProgress: ((Float) -> Unit)? = null,
    onPlayerExpansionSettled: ((Float) -> Unit)? = null,
    onPlayerAction: ((NativePlayerAction) -> Unit)? = null,
    onQueueReordered: ((Int, Int) -> Unit)? = null,
)
