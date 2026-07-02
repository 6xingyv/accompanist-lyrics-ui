package com.mocharealm.accompanist.lyrics.ui.composable.lyrics

import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.viewinterop.AndroidView
import com.mocharealm.accompanist.lyrics.core.model.ISyncedLine
import com.mocharealm.accompanist.lyrics.core.model.SyncedLyrics
import com.mocharealm.accompanist.lyrics.ui.renderer.RustSkiaLyricsView
import com.mocharealm.accompanist.lyrics.ui.renderer.toSceneStyle
import org.jetbrains.compose.resources.FontResource

@Composable
internal actual fun NativeLyricsViewHost(
    lyrics: SyncedLyrics,
    currentPosition: () -> Int,
    onLineClicked: (ISyncedLine) -> Unit,
    onLinePressed: (ISyncedLine) -> Unit,
    modifier: Modifier,
    config: KaraokeLyricsConfig,
    fontResource: FontResource?
) {
    val density = LocalDensity.current
    val fontResourceBytes = rememberFontResourceBytes(fontResource)
    val style = config.toSceneStyle(density)

    AndroidView(
        factory = { context ->
            RustSkiaLyricsView(context).apply {
                configureFonts(fontResourceBytes)
                setStyle(style)
                setLyrics(lyrics)
                setCurrentPosition(currentPosition())
                setLineInteractionCallbacks(
                    onLineClicked = { index -> lyrics.lines.getOrNull(index)?.let(onLineClicked) },
                    onLinePressed = { index -> lyrics.lines.getOrNull(index)?.let(onLinePressed) }
                )
            }
        },
        update = { view ->
            view.configureFonts(fontResourceBytes)
            view.setStyle(style)
            view.setLyrics(lyrics)
            view.setCurrentPosition(currentPosition())
            view.setLineInteractionCallbacks(
                onLineClicked = { index -> lyrics.lines.getOrNull(index)?.let(onLineClicked) },
                onLinePressed = { index -> lyrics.lines.getOrNull(index)?.let(onLinePressed) }
            )
        },
        modifier = modifier.fillMaxSize()
    )
}
