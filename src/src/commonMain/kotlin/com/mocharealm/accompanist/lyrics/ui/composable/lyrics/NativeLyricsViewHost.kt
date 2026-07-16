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
    // `isPlaying`/`backgroundReactive` drive the animation. When `title` is non-null,
    // the surface also draws the player top bar (thumbnail + title/artist + ⋯ button),
    // and `onControlsClick` fires on a ⋯ tap. All ignored off Android.
    backgroundArtwork: ImageBitmap? = null,
    contentPadding: PaddingValues = PaddingValues(0.dp),
    isPlaying: Boolean = true,
    isPlayingUpdates: Flow<Boolean>? = null,
    useMusicFoundationClock: Boolean = false,
    backgroundReactive: Boolean = false,
    title: String? = null,
    artist: String? = null,
    onControlsClick: (() -> Unit)? = null,
)
