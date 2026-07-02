package com.mocharealm.accompanist.lyrics.ui.composable.lyrics

import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.unit.dp
import com.mocharealm.accompanist.lyrics.core.model.ISyncedLine
import com.mocharealm.accompanist.lyrics.core.model.SyncedLyrics
import org.jetbrains.compose.resources.FontResource

@Composable
internal expect fun NativeLyricsViewHost(
    lyrics: SyncedLyrics,
    currentPosition: () -> Int,
    onLineClicked: (ISyncedLine) -> Unit,
    onLinePressed: (ISyncedLine) -> Unit,
    modifier: Modifier,
    config: KaraokeLyricsConfig,
    fontResource: FontResource?,
    // Full-bleed GPU mesh-gradient background (Android). `backgroundArtwork` enables
    // it; `contentPadding` insets the lyrics band below the top bar / above the nav
    // bar; `isPlaying`/`backgroundReactive` drive the animation. Ignored off Android.
    backgroundArtwork: ImageBitmap? = null,
    contentPadding: PaddingValues = PaddingValues(0.dp),
    isPlaying: Boolean = true,
    backgroundReactive: Boolean = false,
)
