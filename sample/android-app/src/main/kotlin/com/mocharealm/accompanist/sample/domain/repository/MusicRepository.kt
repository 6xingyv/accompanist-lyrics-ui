package com.mocharealm.accompanist.sample.domain.repository

import android.net.Uri
import com.mocharealm.accompanist.lyrics.core.model.SyncedLyrics
import com.mocharealm.accompanist.sample.domain.model.MusicItem

interface MusicRepository {
    suspend fun createMusicItem(
        audioUri: Uri,
        lyricsUri: Uri? = null,
        translationUri: Uri? = null
    ): MusicItem

    suspend fun findExternalLyricsPath(audioUri: Uri): String?
    suspend fun getLyricsFor(item: MusicItem): SyncedLyrics?
}
