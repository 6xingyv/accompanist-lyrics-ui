package com.mocharealm.accompanist.sample.domain.repository

import android.net.Uri
import com.mocharealm.accompanist.lyrics.core.model.SyncedLyrics
import com.mocharealm.accompanist.sample.domain.model.MusicItem

data class LocalFileSelection(val audio: Uri, val lyrics: Uri?, val translation: Uri?)

interface MusicRepository {
    suspend fun findExternalLyricsUri(audioUri: Uri): Uri?

    suspend fun lastSelection(): LocalFileSelection?

    suspend fun saveSelection(selection: LocalFileSelection)

    suspend fun createMusicItem(
        audioUri: Uri,
        lyricsUri: Uri? = null,
        translationUri: Uri? = null,
    ): MusicItem

    suspend fun findExternalLyricsPath(audioUri: Uri): String?

    suspend fun getLyricsFor(item: MusicItem): SyncedLyrics?
}
