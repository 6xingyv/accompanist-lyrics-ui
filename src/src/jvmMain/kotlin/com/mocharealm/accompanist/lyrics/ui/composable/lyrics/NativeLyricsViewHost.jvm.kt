package com.mocharealm.accompanist.lyrics.ui.composable.lyrics

import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.awt.SwingPanel
import androidx.compose.ui.platform.LocalDensity
import com.mocharealm.accompanist.lyrics.core.model.ISyncedLine
import com.mocharealm.accompanist.lyrics.core.model.SyncedLyrics
import com.mocharealm.accompanist.lyrics.ui.renderer.RustSkiaLyricsPanel
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

    SwingPanel(
        factory = {
            RustSkiaLyricsPanel().apply {
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
        update = { panel ->
            panel.configureFonts(fontResourceBytes)
            panel.setStyle(style)
            panel.setLyrics(lyrics)
            panel.setCurrentPosition(currentPosition())
            panel.setLineInteractionCallbacks(
                onLineClicked = { index -> lyrics.lines.getOrNull(index)?.let(onLineClicked) },
                onLinePressed = { index -> lyrics.lines.getOrNull(index)?.let(onLinePressed) }
            )
        },
        modifier = modifier.fillMaxSize()
    )
}
