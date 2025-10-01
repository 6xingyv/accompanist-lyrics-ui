package com.mocharealm.accompanist.sample.domain.repository

import com.mocharealm.accompanist.lyrics.core.model.SyncedLyrics
import com.mocharealm.accompanist.sample.domain.model.MusicItem

interface MusicRepository {
    suspend fun getMusicItems(): List<MusicItem>
    suspend fun getLyricsFor(item: MusicItem): SyncedLyrics?
}