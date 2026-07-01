package com.mocharealm.accompanist.lyrics.ui.composable.lyrics

import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import com.mocharealm.accompanist.lyrics.core.model.ISyncedLine
import com.mocharealm.accompanist.lyrics.core.model.SyncedLyrics
import org.jetbrains.compose.resources.FontResource

/**
 * Native lyrics view backed by the Rust renderer.
 *
 * The public composable remains as the Compose entry point for integration with
 * existing screens, but all lyrics layout, hit testing, and drawing are delegated
 * to platform native hosts. Every visual/animation tunable lives in a single
 * [config] object (see [KaraokeLyricsConfig]); `KaraokeLyricsConfig()` reproduces
 * the previous look.
 */
@Suppress("UNUSED_PARAMETER")
@Composable
fun KaraokeLyricsView(
    lyrics: SyncedLyrics,
    currentPosition: () -> Int,
    onLineClicked: (ISyncedLine) -> Unit,
    onLinePressed: (ISyncedLine) -> Unit,
    modifier: Modifier = Modifier,
    config: KaraokeLyricsConfig = KaraokeLyricsConfig(),
    fontResource: FontResource? = null
) {
    NativeLyricsViewHost(
        lyrics = lyrics,
        currentPosition = currentPosition,
        onLineClicked = onLineClicked,
        onLinePressed = onLinePressed,
        modifier = modifier,
        config = config,
        fontResource = fontResource
    )
}
