package com.mocharealm.accompanist.benchmark

import android.content.Intent
import androidx.benchmark.macro.MacrobenchmarkScope
import androidx.test.uiautomator.By
import androidx.test.uiautomator.Configurator
import androidx.test.uiautomator.UiObject2
import androidx.test.uiautomator.Until

internal const val TARGET_PACKAGE = "com.mocharealm.accompanist.demo.benchmark"
private const val HOST = "com.mocharealm.accompanist.sample.performance.LyricsBenchmarkActivity"

internal fun configureAutomation() {
    // Playback intentionally never becomes idle. Readiness uses an explicit layout signal.
    Configurator.getInstance().setWaitForIdleTimeout(0)
}

internal fun MacrobenchmarkScope.launchLyrics() {
    startActivityAndWait(Intent().setClassName(TARGET_PACKAGE, HOST))
    check(device.wait(Until.hasObject(By.res("status").text("ready")), 30_000)) {
        "Lyrics failed to prepare and lay out"
    }
}

internal fun MacrobenchmarkScope.control(tag: String): UiObject2 =
    checkNotNull(device.wait(Until.findObject(By.res(tag)), 5000)) { "Missing control $tag" }

internal fun MacrobenchmarkScope.playLyricsJourney() {
    control("play").click()
    // Actual display frames, including automatic line changes, glow and per-character lift.
    Thread.sleep(6000)
}

internal fun MacrobenchmarkScope.scrollLyricsJourney() {
    val bounds = control("lyrics").visibleBounds
    val x = bounds.centerX()
    repeat(3) {
        device.swipe(x, bounds.bottom - bounds.height() / 5, x, bounds.top + bounds.height() / 5, 24)
        device.swipe(x, bounds.top + bounds.height() / 5, x, bounds.bottom - bounds.height() / 5, 24)
    }
    // Native hit testing routes through LyricsLineItem.onLineClicked, including spring retargets.
    repeat(10) { index ->
        device.click(x, bounds.top + bounds.height() * (if (index % 2 == 0) 1 else 2) / 4)
        Thread.sleep(80)
    }
    control("seek").click()
    Thread.sleep(1200)
    control("seek").click()
    Thread.sleep(1200)
}

internal fun MacrobenchmarkScope.toggleCaptionsJourney() {
    val boundary = control("boundary")
    val phonetic = control("phonetic")
    val translation = control("translation")
    repeat(10) {
        boundary.click()
        phonetic.click()
        translation.click()
    }
    Thread.sleep(1000)
}
