package com.mocharealm.accompanist.lyrics.ui.composable.lyrics

import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.awt.SwingPanel
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import com.mocharealm.accompanist.lyrics.core.model.ISyncedLine
import com.mocharealm.accompanist.lyrics.core.model.SyncedLyrics
import com.mocharealm.accompanist.lyrics.ui.renderer.NativeLyricsRendererStyle
import com.mocharealm.accompanist.lyrics.ui.renderer.RustSkiaLyricsPanel
import org.jetbrains.compose.resources.FontResource

@Composable
internal actual fun NativeLyricsViewHost(
    lyrics: SyncedLyrics,
    currentPosition: () -> Int,
    onLineClicked: (ISyncedLine) -> Unit,
    onLinePressed: (ISyncedLine) -> Unit,
    modifier: Modifier,
    normalLineTextStyle: TextStyle,
    accompanimentLineTextStyle: TextStyle,
    phoneticTextStyle: TextStyle,
    textColor: Color,
    breathingDotsDefaults: KaraokeBreathingDotsDefaults,
    useBlurEffect: Boolean,
    showTranslation: Boolean,
    showPhonetic: Boolean,
    offset: Dp,
    keepAliveZone: Dp,
    blurDelta: Float,
    fontResource: FontResource?
) {
    val density = LocalDensity.current
    val fontResourceBytes = rememberFontResourceBytes(fontResource)
    val style = with(density) {
        NativeLyricsRendererStyle(
            normalFontSizePx = normalLineTextStyle.fontSize.toPx(),
            normalLineHeightPx = normalLineTextStyle.fontSize.toPx() * 1.25f,
            accompanimentFontSizePx = accompanimentLineTextStyle.fontSize.toPx(),
            accompanimentLineHeightPx = accompanimentLineTextStyle.fontSize.toPx() * 1.3f,
            translationFontSizePx = normalLineTextStyle.fontSize.toPx() * 0.46f,
            translationLineHeightPx = normalLineTextStyle.fontSize.toPx() * 0.62f,
            phoneticFontSizePx = phoneticTextStyle.fontSize.toPx(),
            phoneticLineHeightPx = phoneticTextStyle.fontSize.toPx() * 1.25f,
            phoneticGapPx = 4.dp.toPx(),
            paddingXPx = 16.dp.toPx(),
            paddingYPx = 8.dp.toPx(),
            keepAlivePx = offset.toPx() + keepAliveZone.toPx(),
            textColorArgb = textColor.toArgb(),
            showTranslation = showTranslation,
            showPhonetic = showPhonetic,
            useBlurEffect = useBlurEffect,
            blurDelta = blurDelta,
            breathingDotsNumber = breathingDotsDefaults.number,
            breathingDotsSizePx = breathingDotsDefaults.size.toPx(),
            breathingDotsMarginPx = breathingDotsDefaults.margin.toPx(),
            breathingDotsEnterMs = breathingDotsDefaults.enterDurationMs,
            breathingDotsStillMs = breathingDotsDefaults.preExitStillDuration,
            breathingDotsDipMs = breathingDotsDefaults.preExitDipAndRiseDuration,
            breathingDotsExitMs = breathingDotsDefaults.exitDurationMs,
            breathingDotsColorArgb = breathingDotsDefaults.breathingDotsColor.toArgb(),
        )
    }

    SwingPanel(
        factory = {
            RustSkiaLyricsPanel().apply {
                configureFonts(fontResourceBytes)
                setRendererStyle(style)
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
            panel.setRendererStyle(style)
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
