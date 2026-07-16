package com.mocharealm.accompanist.lyrics.ui.composable.lyrics

import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.ImageBitmap
import com.mocharealm.accompanist.lyrics.core.model.ISyncedLine
import com.mocharealm.accompanist.lyrics.core.model.SyncedLyrics
import kotlinx.coroutines.flow.Flow
import org.jetbrains.compose.resources.FontResource

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
}
