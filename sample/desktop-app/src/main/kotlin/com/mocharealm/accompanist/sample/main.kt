package com.mocharealm.accompanist.sample

import androidx.compose.foundation.layout.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Window
import androidx.compose.ui.window.application
import com.mocharealm.accompanist.lyrics.core.model.SyncedLyrics
import com.mocharealm.accompanist.lyrics.core.parser.AutoParser
import com.mocharealm.accompanist.lyrics.ui.composable.list.rememberLyricsLazyListState
import com.mocharealm.accompanist.lyrics.ui.composable.lyrics.KaraokeLyricsView
import java.awt.FileDialog
import java.io.File
import java.util.prefs.Preferences
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

fun main() = application {
    Window(onCloseRequest = ::exitApplication, title = "Accompanist · Local lyrics preview") {
        MaterialTheme(colorScheme = darkColorScheme()) {
            val scope = rememberCoroutineScope()
            var lyrics by remember { mutableStateOf<SyncedLyrics?>(null) }
            var fileName by remember { mutableStateOf<String?>(null) }
            var error by remember { mutableStateOf<String?>(null) }
            var loading by remember { mutableStateOf(false) }
            var playing by remember { mutableStateOf(false) }
            val position = remember { mutableIntStateOf(0) }
            val preferences = remember {
                Preferences.userRoot().node("com/mocharealm/accompanist/sample")
            }
            suspend fun openLyrics(selected: File) {
                loading = true
                playing = false
                error = null
                try {
                    val parsed =
                        withContext(Dispatchers.IO) {
                            AutoParser().parse(selected.readText().removePrefix("\uFEFF")).also {
                                require(it.lines.isNotEmpty()) {
                                    "No supported timed lyrics found in this file."
                                }
                                preferences.put("lastLyrics", selected.absolutePath)
                                preferences.flush()
                            }
                        }
                    lyrics = parsed
                    fileName = selected.name
                    position.intValue = 0
                } catch (failure: Exception) {
                    if (failure is kotlinx.coroutines.CancellationException) throw failure
                    error = failure.message ?: "Unable to open lyrics"
                } finally {
                    loading = false
                }
            }
            LaunchedEffect(Unit) {
                val saved = withContext(Dispatchers.IO) { preferences.get("lastLyrics", null) }
                if (saved != null) openLyrics(File(saved))
            }

            val duration =
                remember(lyrics) { lyrics?.lines?.maxOfOrNull { it.end }?.coerceAtLeast(1) ?: 1 }
            LaunchedEffect(playing, lyrics) {
                if (playing) {
                    var previous = withFrameNanos { it }
                    var remainderNanos = 0L
                    while (playing) {
                        val now = withFrameNanos { it }
                        val elapsed = now - previous + remainderNanos
                        remainderNanos = elapsed % 1_000_000L
                        position.intValue =
                            (position.intValue.toLong() + elapsed / 1_000_000L)
                                .coerceAtMost(duration.toLong())
                                .toInt()
                        previous = now
                        if (position.intValue >= duration) playing = false
                    }
                }
            }
            Surface(Modifier.fillMaxSize()) {
                Column(Modifier.fillMaxSize()) {
                    Row(
                        Modifier.fillMaxWidth().padding(16.dp),
                        horizontalArrangement = Arrangement.spacedBy(12.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Button(
                            enabled = !loading,
                            onClick = {
                                val dialog =
                                    FileDialog(
                                        window,
                                        "Open lyrics (TTML, LRC, ELRC, LYS, KRC)",
                                        FileDialog.LOAD,
                                    )
                                dialog.isVisible = true
                                val selected = dialog.file?.let { File(dialog.directory, it) }
                                dialog.dispose()
                                if (selected != null) scope.launch { openLyrics(selected) }
                            },
                        ) {
                            Text(if (loading) "Opening…" else "Open lyrics")
                        }
                        Button(
                            enabled = lyrics != null,
                            onClick = {
                                if (!playing && position.intValue >= duration) position.intValue = 0
                                playing = !playing
                            },
                        ) {
                            Text(if (playing) "Pause" else "Play preview")
                        }
                        Text(fileName ?: "No file selected")
                    }
                    error?.let {
                        Text(
                            it,
                            color = MaterialTheme.colorScheme.error,
                            modifier = Modifier.padding(horizontal = 16.dp),
                        )
                    }
                    val currentLyrics = lyrics
                    if (currentLyrics == null) {
                        Box(
                            Modifier.weight(1f).fillMaxWidth(),
                            contentAlignment = Alignment.Center,
                        ) {
                            Text("Open a local lyrics file to preview timing and rendering.")
                        }
                    } else {
                        KaraokeLyricsView(
                            listState = rememberLyricsLazyListState(),
                            lyrics = currentLyrics,
                            currentPosition = { position.intValue },
                            onLineClicked = { position.intValue = it.start.coerceIn(0, duration) },
                            onLinePressed = {},
                            modifier = Modifier.weight(1f).fillMaxWidth(),
                            translationTextStyle = LocalTextStyle.current,
                        )
                        PreviewProgress(position, duration)
                    }
                }
            }
        }
    }
}

@Composable
private fun PreviewProgress(position: MutableIntState, duration: Int) {
    Row(
        Modifier.fillMaxWidth().padding(horizontal = 16.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        PreviewTime(position, duration)
        PreviewSlider(position, duration, Modifier.weight(1f).padding(start = 12.dp))
    }
}

@Composable
private fun PreviewTime(position: IntState, duration: Int) {
    val seconds by remember(position) { derivedStateOf { position.intValue / 1000 } }
    Text("${seconds}s / ${duration / 1000}s")
}

@Composable
private fun PreviewSlider(position: MutableIntState, duration: Int, modifier: Modifier) {
    Slider(
        value = position.intValue.toFloat(),
        onValueChange = { position.intValue = it.toInt() },
        valueRange = 0f..duration.toFloat(),
        modifier = modifier,
    )
}
