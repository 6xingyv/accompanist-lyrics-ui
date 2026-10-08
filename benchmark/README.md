# Android performance and Baseline Profiles

`:benchmark` is a separate instrumentation APK. It drives the release-like sample app, including
the actual lyrics library, with Android's normal frame clock and renderer. The target package is
`com.mocharealm.accompanist.demo.benchmark`; it is separate from the regular sample installation.
The lyrics host and automation controls exist only in `benchmarkRelease` and `nonMinifiedRelease`.

## Coverage

| Journey | What it exercises |
| --- | --- |
| Sample cold startup | Real `MainActivity`, application initialization and player UI |
| Lyrics cold load | Preparing and laying out 128 timed, multilingual lines |
| Playback | Progress sweep, character lift, glow, backing vocals and automatic following |
| Scroll and click | Native gestures, repeated row clicks, spring retargets, interludes and texture reuse |
| Caption toggles | Repeated pronunciation and translation changes around line boundaries |

Fixtures are authored in `LyricsBenchmarkFixture.kt`, with Japanese, Chinese, Korean, English
multi-syllable words, Arabic and Hebrew. Lyrics journeys do not require music files, playback
services or storage permissions. The sample startup journey uses the real player, so its data
and permission state should be held constant when comparing results.

## Generate both profiles

Connect one unlocked Android 13/API 33+ device or emulator. Set `ANDROID_SERIAL` when multiple
devices are attached. On Windows use `gradlew.bat` instead of `./gradlew`.

```sh
./gradlew :src:generateBaselineProfile :sample:android-app:generateBaselineProfile \
  -PlyricsSampleAbi=arm64-v8a
```

The same capture feeds both consumers. The library filters rules to
`com.mocharealm.accompanist.lyrics.ui.**`; the sample retains application and dependency rules
and excludes the benchmark host. Generated text profiles belong in version control. Normal
release builds consume these saved profiles without requiring a connected device.

- Library: `src/src/androidMain/generated/baselineProfiles/baseline-prof.txt`
- Sample: `sample/android-app/src/main/generated/baselineProfiles/baseline-prof.txt`
- Sample startup: `sample/android-app/src/main/generated/baselineProfiles/startup-prof.txt`

Generation normally allows up to 15 iterations and looks for three stable iterations. For a
shorter local capture, pass
`'-Pandroid.testInstrumentationRunnerArguments.lyricsProfileMaxIterations=6'`. Reaching that
limit can produce a valid profile before stability is established; use the normal generation
command when refreshing release profiles.

The library profile is packaged into its Android AAR and applies to consuming Android apps.
Baseline Profiles are Android ART optimizations; the desktop/JVM artifact does not use them.
The sample also includes `androidx.profileinstaller`, and R8 rewrites profiles for its release APK.

## Measure performance

Use an unlocked physical device, with a fixed refresh rate, power mode and comparable temperature.
Keep other foreground/background workloads consistent. The complete suite compares
`CompilationMode.None` with `CompilationMode.Partial(BaselineProfileMode.Require)`; a missing
packaged profile fails the latter instead of silently substituting warmup compilation.

```sh
./gradlew :benchmark:connectedBenchmarkReleaseAndroidTest -PlyricsSampleAbi=arm64-v8a \
  '-Pandroid.testInstrumentationRunnerArguments.class=com.mocharealm.accompanist.benchmark.LyricsBenchmark'
```

Each scenario runs five measured iterations for each compilation mode. A one-iteration smoke
run can be requested explicitly:

```sh
./gradlew :benchmark:connectedBenchmarkReleaseAndroidTest -PlyricsSampleAbi=arm64-v8a \
  '-Pandroid.testInstrumentationRunnerArguments.class=com.mocharealm.accompanist.benchmark.LyricsBenchmark' \
  '-Pandroid.testInstrumentationRunnerArguments.lyricsBenchmarkIterations=1'
```

Reports include startup timing, frame CPU duration/deadline overruns, process memory,
`Lyrics.measure`/`Lyrics.rasterizeLine` trace durations and Perfetto captures. Memory metrics
describe process memory, not GPU texture residency. Use the traces to inspect RenderThread/GPU
work and correlate stalls with library sections. Correctness spring/caption stress tests remain
separate from these performance measurements.

Results and traces are under `benchmark/build/outputs/connected_android_test_additional_output/`;
instrumentation reports are under `benchmark/build/reports/androidTests/connected/`.
Use emulator runs to generate profiles or check automation, not as device performance numbers.
For emulator smoke runs only, explicitly pass
`'-Pandroid.testInstrumentationRunnerArguments.androidx.benchmark.suppressErrors=EMULATOR'`.
If a device rejects input injection with `INJECT_EVENTS`, its system must permit UiAutomator
cross-app automation before interactive benchmarks can run.

Reference: [Android Baseline Profile configuration](https://developer.android.com/topic/performance/baselineprofiles/configure-baselineprofiles).
