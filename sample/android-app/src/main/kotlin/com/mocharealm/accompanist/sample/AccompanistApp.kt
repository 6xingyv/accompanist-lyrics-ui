package com.mocharealm.accompanist.sample

import android.app.Application
import com.mocharealm.accompanist.sample.di.dataModule
import com.mocharealm.accompanist.sample.di.uiModule
import org.koin.android.ext.koin.androidContext
import org.koin.core.context.startKoin

class AccompanistApp : Application() {
    override fun onCreate() {
        super.onCreate()
        if (applicationInfo.flags and android.content.pm.ApplicationInfo.FLAG_DEBUGGABLE != 0) {
            AndroidLyricsSpringTraceRecorder.start(this)
        }
        startKoin {
            modules(dataModule, uiModule)
            androidContext(this@AccompanistApp)
        }
    }
}
