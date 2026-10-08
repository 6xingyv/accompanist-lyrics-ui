package com.mocharealm.accompanist.sample.performance

import android.os.Bundle
import android.view.WindowManager
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.material3.Button
import androidx.compose.material3.Text
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.testTagsAsResourceId
import androidx.compose.ui.unit.dp
import com.mocharealm.accompanist.lyrics.ui.composable.list.LyricsLazyListState
import com.mocharealm.accompanist.lyrics.ui.composable.lyrics.KaraokeLyricsView
import com.mocharealm.accompanist.sample.ui.theme.AccompanistTheme
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.isActive

/** Only present in benchmarkRelease/nonMinifiedRelease; uses the real Android frame clock. */
class LyricsBenchmarkActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
        setContent {
            AccompanistTheme {
                val lyrics = remember { LyricsBenchmarkFixture.create() }
                val state = remember { LyricsLazyListState() }
                var position by remember { mutableIntStateOf(lyrics.lines[1].start + 100) }
                var playing by remember { mutableStateOf(false) }
                var phonetic by remember { mutableStateOf(true) }
                var translation by remember { mutableStateOf(true) }
                var ready by remember { mutableStateOf(false) }
                LaunchedEffect(playing) {
                    if (!playing) return@LaunchedEffect
                    var last = withFrameNanos { it }
                    var remainder = 0L
                    while (isActive) {
                        val now = withFrameNanos { it }
                        val elapsed = now - last + remainder
                        position += (elapsed / 1_000_000).toInt()
                        remainder = elapsed % 1_000_000
                        last = now
                    }
                }
                LaunchedEffect(state) {
                    snapshotFlow { state.canScrollForward }.first { it }
                    repeat(2) { withFrameNanos { } }
                    ready = true
                    reportFullyDrawn()
                }
                Column(Modifier.fillMaxSize().background(Color(0xff252b50))
                    .windowInsetsPadding(WindowInsets.systemBars)
                    .semantics { testTagsAsResourceId = true }) {
                    Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceEvenly) {
                        Button({ playing = !playing }, Modifier.testTag("play")) { Text("Play") }
                        Button({ phonetic = !phonetic }, Modifier.testTag("phonetic")) { Text("读音") }
                        Button({ translation = !translation }, Modifier.testTag("translation")) { Text("翻译") }
                    }
                    Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceEvenly) {
                        Button({
                            val line = lyrics.lines.firstOrNull { position <= it.end } ?: lyrics.lines[1]
                            position = line.end - 120
                            playing = true
                        }, Modifier.testTag("boundary")) { Text("换行") }
                        Button({
                            // Seek across the interlude, then revisit cached rows.
                            position = if (position < lyrics.lines[8].start) lyrics.lines[8].start - 1000
                                else lyrics.lines[1].start + 100
                        }, Modifier.testTag("seek")) { Text("Seek") }
                        Text(if (ready) "ready" else "loading", Modifier.padding(8.dp).testTag("status"))
                    }
                    KaraokeLyricsView(state, lyrics, { position }, { position = it.start }, {},
                        modifier = Modifier.weight(1f).fillMaxWidth().testTag("lyrics"),
                        showPhonetic = phonetic, showTranslation = translation,
                        autoScrollResumeDelayMillis = 300)
                }
            }
        }
    }
}
