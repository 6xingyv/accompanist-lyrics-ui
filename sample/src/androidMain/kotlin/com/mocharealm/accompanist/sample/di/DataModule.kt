package com.mocharealm.accompanist.sample.di

import com.mocharealm.accompanist.sample.data.repository.MusicRepositoryImpl
import com.mocharealm.accompanist.sample.domain.repository.MusicRepository
import org.koin.dsl.module

val dataModule = module {
    single<MusicRepository> {
        MusicRepositoryImpl(get())
    }
}