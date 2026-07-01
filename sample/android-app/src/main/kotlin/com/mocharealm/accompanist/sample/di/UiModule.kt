package com.mocharealm.accompanist.sample.di

import android.content.ComponentName
import android.content.Context
import androidx.media3.session.MediaController
import androidx.media3.session.SessionToken
import com.mocharealm.accompanist.sample.domain.repository.MusicRepository
import com.mocharealm.accompanist.sample.service.PlaybackService
import com.mocharealm.accompanist.sample.ui.screen.player.PlayerViewModel
import com.mocharealm.accompanist.sample.ui.screen.share.ShareViewModel
import org.koin.core.module.dsl.viewModel
import org.koin.dsl.module

val uiModule = module {
    viewModel {
        val context = get<Context>()

        val sessionToken =
            SessionToken(context, ComponentName(context, PlaybackService::class.java))
        val future = MediaController.Builder(context, sessionToken).buildAsync()
        PlayerViewModel(get<MusicRepository>(), future)
    }
    viewModel {
        ShareViewModel()
    }
}