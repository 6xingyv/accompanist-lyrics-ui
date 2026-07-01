package com.mocharealm.accompanist.sample.domain.model

import android.net.Uri
import androidx.media3.common.MediaItem

data class MusicItem(
    val label: String,
    val artist: String,
    val titleFromAudioMetadata: Boolean = false,
    val artistFromAudioMetadata: Boolean = false,
    val audioPath: String,
    val uri: Uri = Uri.fromFile(java.io.File(audioPath)),
    val lyricsPath: String? = null,
    val translationPath: String? = null,
    val lyricsUri: Uri? = null,
    val translationUri: Uri? = null,
    val mediaItem: MediaItem = MediaItem.fromUri(uri)
)
