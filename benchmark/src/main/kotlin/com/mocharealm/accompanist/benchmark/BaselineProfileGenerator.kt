package com.mocharealm.accompanist.benchmark

import androidx.benchmark.macro.junit4.BaselineProfileRule
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class BaselineProfileGenerator {
    @get:Rule val rule = BaselineProfileRule()
    private val maxIterations = InstrumentationRegistry.getArguments()
        .getString("lyricsProfileMaxIterations", "15").toInt().also { require(it >= 3) }

    @Before fun configure() = configureAutomation()

    @Test fun startup() = rule.collect(TARGET_PACKAGE, maxIterations = maxIterations,
        includeInStartupProfile = true) {
        pressHome()
        startActivityAndWait()
    }

    @Test fun lyrics() = rule.collect(TARGET_PACKAGE, maxIterations = maxIterations,
        includeInStartupProfile = false) {
        launchLyrics()
        playLyricsJourney()
        scrollLyricsJourney()
        toggleCaptionsJourney()
    }
}
