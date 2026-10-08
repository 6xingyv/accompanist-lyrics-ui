package com.mocharealm.accompanist.benchmark

import androidx.benchmark.macro.*
import androidx.benchmark.macro.junit4.MacrobenchmarkRule
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Rule
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.junit.runners.Parameterized

/** Same workload with and without AOT compilation from the packaged Baseline Profile. */
@RunWith(Parameterized::class)
@OptIn(ExperimentalMetricApi::class)
class LyricsBenchmark(private val compilation: CompilationMode) {
    @get:Rule val rule = MacrobenchmarkRule()
    private val iterations = InstrumentationRegistry.getArguments()
        .getString("lyricsBenchmarkIterations", "5").toInt().also { require(it > 0) }

    @Before fun configure() = configureAutomation()

    @Test fun sampleColdStartup() = rule.measureRepeated(
        packageName = TARGET_PACKAGE,
        metrics = listOf(StartupTimingMetric()),
        compilationMode = compilation,
        startupMode = StartupMode.COLD,
        iterations = iterations,
        setupBlock = { pressHome() },
    ) { startActivityAndWait() }

    @Test fun lyricsColdLoad() = rule.measureRepeated(
        packageName = TARGET_PACKAGE,
        metrics = listOf(StartupTimingMetric(), FrameTimingMetric(), MemoryUsageMetric(MemoryUsageMetric.Mode.Last)),
        compilationMode = compilation,
        startupMode = StartupMode.COLD,
        iterations = iterations,
        setupBlock = { pressHome() },
    ) { launchLyrics() }

    @Test fun playback() = measure { playLyricsJourney() }
    @Test fun scrollAndClick() = measure { scrollLyricsJourney() }
    @Test fun captionToggles() = measure { toggleCaptionsJourney() }

    private fun measure(journey: MacrobenchmarkScope.() -> Unit) = rule.measureRepeated(
        packageName = TARGET_PACKAGE,
        metrics = listOf(
            FrameTimingMetric(), MemoryUsageMetric(MemoryUsageMetric.Mode.Last), ArtMetric(),
            TraceSectionMetric("Lyrics.measure", TraceSectionMetric.Mode.Sum),
            TraceSectionMetric("Lyrics.rasterizeLine", TraceSectionMetric.Mode.Sum),
        ),
        compilationMode = compilation,
        iterations = iterations,
        setupBlock = { killProcess(); launchLyrics(); Thread.sleep(1000) },
        measureBlock = journey,
    )

    companion object {
        @JvmStatic @Parameterized.Parameters(name = "{0}")
        fun compilationModes() = listOf(
            arrayOf<Any>(CompilationMode.None()),
            arrayOf<Any>(CompilationMode.Partial(baselineProfileMode = BaselineProfileMode.Require)),
        )
    }
}
