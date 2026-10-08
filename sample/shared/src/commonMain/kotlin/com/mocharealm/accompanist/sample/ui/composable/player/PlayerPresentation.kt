package com.mocharealm.accompanist.sample.ui.composable.player

import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.Icon
import androidx.compose.material3.LocalTextStyle
import androidx.compose.material3.Text
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Slider
import androidx.compose.material3.TextButton
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.*
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextMotion
import androidx.compose.ui.unit.TextUnit
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.mocharealm.accompanist.sample.Res
import com.mocharealm.accompanist.sample.empty
import com.mocharealm.accompanist.sample.ic_ellipsis
import com.mocharealm.accompanist.sample.ic_phonetic
import com.mocharealm.accompanist.sample.ic_translation
import com.mocharealm.accompanist.sample.ui.theme.SFPro
import com.mocharealm.accompanist.lyrics.core.model.ISyncedLine
import com.mocharealm.accompanist.lyrics.core.model.SyncedLyrics
import com.mocharealm.accompanist.lyrics.ui.composable.list.LyricsLazyListState
import com.mocharealm.accompanist.lyrics.ui.composable.lyrics.KaraokeLyricsView
import com.mocharealm.accompanist.lyrics.ui.composable.lyrics.LyricsAnchor
import com.mocharealm.accompanist.lyrics.ui.composable.lyrics.LyricsFade
import com.mocharealm.accompanist.sample.ui.composable.background.BackgroundVisualState
import com.mocharealm.accompanist.sample.ui.composable.background.FlowingLightBackground
import org.jetbrains.compose.resources.painterResource
import org.jetbrains.compose.resources.imageResource

@Immutable
data class PlayerLyricsTypography(
    val normal: TextStyle,
    val accompaniment: TextStyle,
    val translation: TextStyle,
    val phonetic: TextStyle,
)

/** Shared metrics; desktop may scale the lyrics with its window dimensions. */
@Composable
fun rememberPlayerLyricsTypography(fontScale: Float = 1f): PlayerLyricsTypography {
    val base = LocalTextStyle.current
    val family = SFPro()
    return remember(base, family, fontScale) {
        PlayerLyricsTypography(
            base.copy(fontSize = (34 * fontScale).sp, lineHeight = TextUnit.Unspecified,
                fontFamily = family, fontWeight = FontWeight.Bold, textMotion = TextMotion.Animated),
            base.copy(fontSize = (20 * fontScale).sp, lineHeight = TextUnit.Unspecified,
                fontFamily = family, fontWeight = FontWeight.Bold, textMotion = TextMotion.Animated),
            base.copy(fontSize = (18 * fontScale).sp, lineHeight = TextUnit.Unspecified,
                fontFamily = family, fontWeight = FontWeight.Bold, textMotion = TextMotion.Animated),
            base.copy(fontSize = (18 * fontScale).sp, lineHeight = TextUnit.Unspecified,
                fontFamily = family, fontWeight = FontWeight.SemiBold, textMotion = TextMotion.Static),
        )
    }
}

@Composable
fun PlayerSurface(
    background: BackgroundVisualState,
    modifier: Modifier = Modifier,
    content: @Composable BoxScope.() -> Unit,
) {
    Box(modifier.fillMaxSize()) {
        FlowingLightBackground(background)
        content()
    }
}

@Composable
fun PlayerCover(bitmap: ImageBitmap?, modifier: Modifier = Modifier) {
    Image(bitmap ?: imageResource(Res.drawable.empty), "Album artwork", modifier)
}

@Composable
fun PlayerHeader(
    title: String,
    artist: String,
    modifier: Modifier = Modifier,
    artwork: (@Composable () -> Unit)? = null,
    controls: @Composable () -> Unit,
    metadataTimeMillis: () -> Int = { 0 },
) {
    Row(modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
        Row(Modifier.weight(1f), horizontalArrangement = Arrangement.spacedBy(8.dp),
            verticalAlignment = Alignment.CenterVertically) {
            artwork?.invoke()
            PlayerMetadata(title, artist, Modifier.weight(1f), metadataTimeMillis)
        }
        Spacer(Modifier.width(8.dp))
        controls()
    }
}

/** Platform callbacks own playback and sharing; presentation has one implementation. */
@Composable
fun PlayerLyricsPanel(
    listState: LyricsLazyListState,
    lyrics: SyncedLyrics?,
    currentPosition: () -> Int,
    showTranslation: Boolean,
    showPhonetic: Boolean,
    onLineClicked: (ISyncedLine) -> Unit,
    modifier: Modifier = Modifier,
    onLinePressed: (ISyncedLine) -> Unit = {},
    anchor: LyricsAnchor = LyricsAnchor.Fixed(64.dp),
    bottomFade: LyricsFade = LyricsFade.Fraction(0.5f),
    loading: Boolean = false,
    emptyMessage: String = "No timed lyrics loaded. Open local audio and lyrics to begin.",
    fontScale: Float = 1f,
) {
    val typography = rememberPlayerLyricsTypography(fontScale)
    if (lyrics == null) {
        Box(modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
            if (loading) CircularProgressIndicator()
            else Text(emptyMessage, color = Color.White.copy(alpha = 0.6f),
                modifier = Modifier.padding(24.dp))
        }
    } else {
        KaraokeLyricsView(listState, lyrics, currentPosition, onLineClicked, onLinePressed,
            modifier = modifier, anchor = anchor, bottomFade = bottomFade,
            showTranslation = showTranslation, showPhonetic = showPhonetic,
            normalLineTextStyle = typography.normal,
            accompanimentLineTextStyle = typography.accompaniment,
            translationTextStyle = typography.translation,
            phoneticTextStyle = typography.phonetic, useBlurEffect = true)
    }
}

/** Keep the ticking position inside this component rather than recomposing the player screen. */
@Composable
fun PlayerProgress(position: IntState, duration: Long, canSeek: Boolean, seek: (Long) -> Unit) {
    var dragged by remember { mutableStateOf<Float?>(null) }
    val maximum = duration.coerceIn(1, Int.MAX_VALUE.toLong()).toFloat()
    Column(Modifier.fillMaxWidth().padding(horizontal = 16.dp)) {
        Slider(value = dragged ?: position.intValue.toFloat().coerceIn(0f, maximum),
            onValueChange = { dragged = it },
            onValueChangeFinished = { dragged?.let { seek(it.toLong()) }; dragged = null },
            enabled = canSeek, valueRange = 0f..maximum)
        val seconds by remember(position) { derivedStateOf { position.intValue / 1000 } }
        Text("${seconds / 60}:${(seconds % 60).toString().padStart(2, '0')} / " +
            "${duration / 60_000}:${(duration / 1000 % 60).toString().padStart(2, '0')}",
            color = Color.White.copy(alpha = .6f), fontSize = 11.sp)
    }
}

@Composable
fun PlayerTransport(playing: Boolean, enabled: Boolean, toggle: () -> Unit,
    status: String, playLabel: String = "Play") {
    Row(Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 4.dp),
        verticalAlignment = Alignment.CenterVertically) {
        TextButton(toggle, enabled = enabled) { Text(if (playing) "Pause" else playLabel) }
        Text(status, fontSize = 11.sp, color = Color.White.copy(alpha = .5f),
            modifier = Modifier.weight(1f))
    }
}

@Composable
fun PlayerMetadata(title: String, artist: String, modifier: Modifier = Modifier,
    currentTimeMillis: () -> Int = { 0 }) {
    val base = LocalTextStyle.current.copy(fontFamily = SFPro(), letterSpacing = 0.sp,
        textMotion = TextMotion.Animated)
    // Each line owns its Plus layer, so the fade can extend into adjacent padding.
    Column(modifier) {
        PlayerMarqueeText(title, base.copy(fontSize = 17.sp, lineHeight = (17 * 1.3f).sp,
            fontWeight = FontWeight.SemiBold), 1f, currentTimeMillis)
        PlayerMarqueeText(artist, base.copy(fontSize = 15.sp, lineHeight = (15 * 1.3f).sp,
            fontWeight = FontWeight.Normal), .4f, currentTimeMillis)
    }
}

@Composable
fun PlayerControls(
    onOpenSongSelection: () -> Unit,
    showTranslation: Boolean,
    showPhonetic: Boolean,
    onToggleTranslation: () -> Unit,
    onTogglePhonetic: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Row(modifier.graphicsLayer { blendMode = BlendMode.Plus },
        horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
        Box(Modifier.clip(CircleShape)
            .background(Color.White.copy(if (showTranslation) 0.6f else 0.2f))
            .clickable(onClick = onToggleTranslation).padding(4.dp)) {
            Icon(painterResource(Res.drawable.ic_translation), "Translation",
                Modifier.size(20.dp).align(Alignment.Center).graphicsLayer {
                    if (showTranslation) blendMode = BlendMode.DstOut
                }, tint = Color.White)
        }
        Box(Modifier.clip(CircleShape)
            .background(Color.White.copy(if (showPhonetic) 0.6f else 0.2f))
            .clickable(onClick = onTogglePhonetic).padding(4.dp)) {
            Icon(painterResource(Res.drawable.ic_phonetic), "Pronunciation",
                Modifier.size(20.dp).align(Alignment.Center).graphicsLayer {
                    if (showPhonetic) blendMode = BlendMode.DstOut
                }, tint = Color.White)
        }
        Box(Modifier.clip(CircleShape).background(Color.White.copy(0.2f))
            .clickable(onClick = onOpenSongSelection).padding(4.dp)) {
            Icon(painterResource(Res.drawable.ic_ellipsis), "Open files",
                Modifier.size(20.dp).align(Alignment.Center), tint = Color.White)
        }
    }
}
