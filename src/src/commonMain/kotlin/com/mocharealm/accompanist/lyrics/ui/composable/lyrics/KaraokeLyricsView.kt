package com.mocharealm.accompanist.lyrics.ui.composable.lyrics

import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.material3.LocalTextStyle
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.BlendMode
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextMotion
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.mocharealm.accompanist.lyrics.core.model.ISyncedLine
import com.mocharealm.accompanist.lyrics.core.model.SyncedLyrics
import org.jetbrains.compose.resources.FontResource

/**
 * Native lyrics view backed by the Rust renderer.
 *
 * The public composable remains as the Compose entry point for integration with
 * existing screens, but all lyrics layout, hit testing, and drawing are delegated
 * to platform native hosts.
 */
@Suppress("UNUSED_PARAMETER")
@Composable
fun KaraokeLyricsView(
    listState: LazyListState,
    lyrics: SyncedLyrics,
    currentPosition: () -> Int,
    onLineClicked: (ISyncedLine) -> Unit,
    onLinePressed: (ISyncedLine) -> Unit,
    modifier: Modifier = Modifier,
    normalLineTextStyle: TextStyle = LocalTextStyle.current.copy(
        fontSize = 34.sp,
        fontWeight = FontWeight.Bold,
        textMotion = TextMotion.Animated,
    ),
    accompanimentLineTextStyle: TextStyle = LocalTextStyle.current.copy(
        fontSize = 20.sp,
        fontWeight = FontWeight.Bold,
        textMotion = TextMotion.Animated,
    ),
    textColor: Color = Color.White,
    breathingDotsDefaults: KaraokeBreathingDotsDefaults = KaraokeBreathingDotsDefaults(),
    phoneticTextStyle: TextStyle = normalLineTextStyle.copy(
        fontSize = 13.sp,
        fontWeight = FontWeight.Normal,
    ),
    blendMode: BlendMode = BlendMode.Plus,
    useBlurEffect: Boolean = true,
    showTranslation: Boolean = true,
    showPhonetic: Boolean = true,
    offset: Dp = 32.dp,
    keepAliveZone: Dp = 100.dp,
    blurDelta: Float = 3f,
    showDebugRectangles: Boolean = false,
    fontResource: FontResource? = null
) {
    NativeLyricsViewHost(
        lyrics = lyrics,
        currentPosition = currentPosition,
        onLineClicked = onLineClicked,
        onLinePressed = onLinePressed,
        modifier = modifier,
        normalLineTextStyle = normalLineTextStyle,
        accompanimentLineTextStyle = accompanimentLineTextStyle,
        phoneticTextStyle = phoneticTextStyle,
        textColor = textColor,
        breathingDotsDefaults = breathingDotsDefaults,
        useBlurEffect = useBlurEffect,
        showTranslation = showTranslation,
        showPhonetic = showPhonetic,
        offset = offset,
        keepAliveZone = keepAliveZone,
        blurDelta = blurDelta,
        fontResource = fontResource
    )
}
