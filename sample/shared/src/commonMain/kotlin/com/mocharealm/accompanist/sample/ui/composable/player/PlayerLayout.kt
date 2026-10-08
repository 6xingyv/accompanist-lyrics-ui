package com.mocharealm.accompanist.sample.ui.composable.player

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.animateContentSize
import androidx.compose.foundation.layout.*
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.unit.dp
import com.mocharealm.accompanist.lyrics.ui.composable.lyrics.LyricsAnchor
import com.mocharealm.accompanist.lyrics.ui.composable.lyrics.LyricsFade
import com.mocharealm.gaze.capsule.ContinuousRoundedRectangle

/** Android sample geometry, shared by every platform below its system/window chrome. */
@Composable
fun PlayerLayout(
    title: String,
    artist: String,
    hasArtwork: Boolean,
    artwork: @Composable (Modifier) -> Unit,
    controls: @Composable () -> Unit,
    lyrics: @Composable (Modifier, LyricsAnchor, LyricsFade) -> Unit,
    modifier: Modifier = Modifier,
    compact: Boolean? = null,
    showLyricsInWideLayout: Boolean = true,
    metadataTimeMillis: () -> Int = { 0 },
) {
    BoxWithConstraints(modifier.fillMaxSize()) {
        if (compact ?: (maxWidth < 600.dp)) {
            Column(Modifier.fillMaxSize()) {
                PlayerHeader(
                    title, artist,
                    modifier = Modifier.padding(horizontal = 28.dp).padding(top = 28.dp),
                    artwork = if (hasArtwork) {
                        { artwork(Modifier.clip(ContinuousRoundedRectangle(12.dp)).size(72.dp)) }
                    } else null,
                    controls = controls,
                    metadataTimeMillis = metadataTimeMillis,
                )
                lyrics(
                    Modifier.weight(1f).fillMaxWidth().padding(horizontal = 12.dp),
                    LyricsAnchor.Fixed(64.dp),
                    LyricsFade.Fraction(0.5f),
                )
            }
        } else {
            Row(Modifier.fillMaxSize().animateContentSize(),
                horizontalArrangement = Arrangement.Center) {
                Column(
                    Modifier.fillMaxWidth(0.4f).fillMaxHeight()
                        .padding(start = 100.dp, top = 28.dp),
                    horizontalAlignment = Alignment.CenterHorizontally,
                    verticalArrangement = Arrangement.Center,
                ) {
                    artwork(Modifier.clip(ContinuousRoundedRectangle(12.dp))
                        .fillMaxWidth().aspectRatio(1f))
                    Spacer(Modifier.height(28.dp))
                    PlayerHeader(title, artist, controls = controls, metadataTimeMillis = metadataTimeMillis)
                }
                AnimatedVisibility(showLyricsInWideLayout, modifier = Modifier.weight(1f)) {
                    lyrics(
                        Modifier.fillMaxSize().padding(horizontal = 12.dp)
                            .padding(start = 60.dp, end = 60.dp),
                        LyricsAnchor.Fraction(0.4f),
                        LyricsFade.Fraction(0.2f),
                    )
                }
            }
        }
    }
}
