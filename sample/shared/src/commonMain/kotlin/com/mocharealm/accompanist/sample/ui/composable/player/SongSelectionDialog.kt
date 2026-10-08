package com.mocharealm.accompanist.sample.ui.composable.player

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp

/** One dialog presentation; each platform owns its document pickers and import pipeline. */
@Composable
fun SongSelectionDialogContent(
    audioName: String,
    lyricsName: String,
    translationName: String,
    audioSelected: Boolean,
    isImporting: Boolean,
    isReady: Boolean,
    error: String?,
    onSelectAudio: () -> Unit,
    onSelectLyrics: () -> Unit,
    onSelectTranslation: () -> Unit,
    onPlay: () -> Unit,
    onDismissRequest: () -> Unit,
    fileAccessMessage: String? = null,
) {
    AlertDialog(
        onDismissRequest = onDismissRequest,
        title = { Text("Open local files") },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                listOf(audioName to onSelectAudio, lyricsName to onSelectLyrics,
                    translationName to onSelectTranslation).forEach { (name, select) ->
                    OutlinedButton(onClick = select, enabled = !isImporting,
                        modifier = Modifier.fillMaxWidth()) { Text(name) }
                }
                fileAccessMessage?.let { Text(it) }
                error?.let { Text(it, color = MaterialTheme.colorScheme.error) }
            }
        },
        confirmButton = {
            Button(onClick = onPlay, enabled = audioSelected && isReady && !isImporting) {
                Text(if (isImporting) "Opening…" else "Play")
            }
        },
        dismissButton = { Button(onClick = onDismissRequest) { Text("Cancel") } },
    )
}
