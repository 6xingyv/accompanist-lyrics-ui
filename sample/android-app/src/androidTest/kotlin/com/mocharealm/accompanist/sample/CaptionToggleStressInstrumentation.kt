package com.mocharealm.accompanist.sample

import android.app.Activity
import android.app.Instrumentation
import android.os.Bundle
import android.os.ParcelFileDescriptor
import android.os.Looper
import android.util.Log
import android.graphics.Bitmap
import android.view.View
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.*
import androidx.compose.runtime.snapshots.Snapshot
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.ComposeView
import com.mocharealm.accompanist.lyrics.core.model.karaoke.KaraokeLine
import com.mocharealm.accompanist.lyrics.core.model.synced.SyncedLine
import com.mocharealm.accompanist.lyrics.core.parser.TTMLParser
import com.mocharealm.accompanist.lyrics.ui.composable.list.LyricsLazyListState
import com.mocharealm.accompanist.lyrics.ui.composable.lyrics.KaraokeLyricsView
import com.mocharealm.accompanist.lyrics.ui.diagnostics.LyricsSpringTrace
import com.mocharealm.accompanist.sample.data.utils.AndroidPhoneticProvider
import com.mocharealm.accompanist.sample.ui.theme.AccompanistTheme
import java.io.File
import java.nio.ByteBuffer
import java.nio.ByteOrder
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicReference
import kotlin.concurrent.thread
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.launch

/**
 * Android placement/graphics-layer stress test driven by an explicit frame clock. Uses real
 * AndroidComposeView nodes and RectManager instead of desktop scenes. The process must remain
 * running; some devices freeze every thread while locked, including instrumentation threads.
 * Build with -PtestInstrumentationRunner=com.mocharealm.accompanist.sample.CaptionToggleStressInstrumentation.
 * Supply TTML files in the target app's cache/caption-stress directory, then invoke am instrument.
 * The empty activity host only exists in debug builds; release builds have no test hooks.
 */
class CaptionToggleStressInstrumentation : Instrumentation() {
    override fun onCreate(arguments: Bundle?) {
        super.onCreate(arguments)
        start()
    }

    override fun onStart() {
        var activity: CaptionToggleStressActivity? = null
        val stage = AtomicReference("launch")
        val watchdog = thread(name = "caption-stress-watchdog", isDaemon = true) {
            try {
                Thread.sleep(15_000)
                Log.i(TAG, "Headless stage=${stage.get()}; main stack:\n" +
                    Looper.getMainLooper().thread.stackTrace.joinToString("\n"))
            } catch (_: InterruptedException) { }
        }
        try {
            val fixtureDirectory = File(targetContext.cacheDir, "caption-stress")
            val files = fixtureDirectory
                .listFiles { file -> file.extension.equals("ttml", ignoreCase = true) }
                ?.sortedBy { it.name }.orEmpty()
            check(files.isNotEmpty()) { "No TTML fixtures in $fixtureDirectory" }
            Log.i(TAG, "Launching stress host with ${files.size} fixtures")
            val monitor = addMonitor(CaptionToggleStressActivity::class.java.name, null, false)
            try {
                // Xiaomi blocks starts from the instrumented app while it is in the background.
                // Let the test's shell automation launch its own host activity instead.
                val command = uiAutomation.executeShellCommand(
                    "am start -n ${targetContext.packageName}/${CaptionToggleStressActivity::class.java.name}")
                ParcelFileDescriptor.AutoCloseInputStream(command).bufferedReader().use { it.readText() }
                activity = waitForMonitorWithTimeout(monitor, 20_000) as? CaptionToggleStressActivity
                    ?: error("Stress host did not start; unlock the device before running")
            } finally {
                removeMonitor(monitor)
            }
            Log.i(TAG, "Stress host launched")
            var totalToggles = 0
            val reports = mutableListOf<String>()
            for (file in files) {
                Log.i(TAG, "Parsing ${file.name}")
                val lyrics = TTMLParser(AndroidPhoneticProvider).parse(file.readText())
                val main = lyrics.lines.filter { it is KaraokeLine.MainKaraokeLine || it is SyncedLine }
                check(main.size >= 3) { "${file.name}: fixture needs at least three timed lines" }
                val completed = CountDownLatch(1)
                var failure: Throwable? = null
                var report = ""
                val host = activity
                val clock = BroadcastFrameClock()
                val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate + clock)
                val recomposer = Recomposer(scope.coroutineContext)
                lateinit var root: ComposeView
                val bitmap = Bitmap.createBitmap(1080, 2400, Bitmap.Config.ARGB_8888)
                val canvas = android.graphics.Canvas(bitmap)
                runOnMainSync {
                    stage.set("composition ${file.name}")
                    root = ComposeView(host)
                    // Keep the real native owner attached for measure/placement, while the
                    // test's sampled software draws own rendering on a headless emulator.
                    root.visibility = View.INVISIBLE
                    root.setParentCompositionContext(recomposer)
                    host.setContentView(root)
                    scope.launch { recomposer.runRecomposeAndApplyChanges() }
                    root.setContent {
                        AccompanistTheme {
                            val state = remember(file) { LyricsLazyListState() }
                            var time by remember(file) { mutableIntStateOf(main[1].end - 240) }
                            var phonetic by remember(file) { mutableStateOf(true) }
                            var translation by remember(file) { mutableStateOf(true) }
                            KaraokeLyricsView(state, lyrics, { time }, {}, {},
                                modifier = Modifier.fillMaxSize(), showPhonetic = phonetic,
                                showTranslation = translation, autoScrollResumeDelayMillis = 0)
                            LaunchedEffect(file) {
                                var framesWithRows = 0
                                var animationStarts = 0
                                var retargets = 0
                                var toggles = 0
                                var phase = 0
                                LyricsSpringTrace.install { record ->
                                    val data = ByteBuffer.wrap(record).order(ByteOrder.LITTLE_ENDIAN)
                                    if (data.getInt(94) > 0) framesWithRows++
                                    if (phase == 1 && data.getInt(21) == 112) {
                                        animationStarts++
                                        if (data.getFloat(45) != 0f) retargets++
                                    }
                                }
                                try {
                                    var warmup = 0
                                    while (framesWithRows == 0 && warmup++ < 600) { withFrameNanos { } }
                                    check(framesWithRows > 0) { "Lyrics never laid out" }
                                    repeat(100) { withFrameNanos { } }
                                    phase = 1
                                    // Hold near the boundary, then cross it while toggling every two
                                    // display frames. Keep real Android measure/place/draw active.
                                    repeat(120) { frame ->
                                        withFrameNanos { }
                                        if (frame % 2 == 0) { phonetic = !phonetic; toggles++ }
                                    }
                                    repeat(320) { frame ->
                                        withFrameNanos { }
                                        time += 16
                                        if (frame % 2 == 0) { phonetic = !phonetic; toggles++ }
                                    }
                                    // Also exercise translation and pronunciation sharing reveal.
                                    repeat(120) { frame ->
                                        withFrameNanos { }
                                        time += 16
                                        if (frame % 2 == 0) {
                                            phonetic = !phonetic
                                            translation = !translation
                                            toggles++
                                        }
                                    }
                                    phase = 2
                                    phonetic = false
                                    translation = false
                                    repeat(120) { withFrameNanos { } }
                                    report = "PASS ${file.name}: toggles=$toggles starts=$animationStarts retargets=$retargets rows=$framesWithRows"
                                    Log.i(TAG, report)
                                    totalToggles += toggles
                                } catch (error: Throwable) {
                                    failure = error
                                    Log.e(TAG, "FAIL ${file.name}", error)
                                } finally {
                                    LyricsSpringTrace.clear()
                                    completed.countDown()
                                }
                            }
                        }
                    }
                }
                try {
                    var frame = 0L
                    val deadline = System.nanoTime() + TimeUnit.SECONDS.toNanos(180)
                    while (completed.count != 0L && frame < 2000 && System.nanoTime() < deadline) {
                        runOnMainSync {
                            stage.set("frame $frame clock")
                            Snapshot.sendApplyNotifications()
                            clock.sendFrame(++frame * 16_000_000L)
                            stage.set("frame $frame measure")
                            root.forceLayout()
                            root.measure(View.MeasureSpec.makeMeasureSpec(1080, View.MeasureSpec.EXACTLY),
                                View.MeasureSpec.makeMeasureSpec(2400, View.MeasureSpec.EXACTLY))
                            root.layout(0, 0, 1080, 2400)
                            stage.set("frame $frame draw")
                            // Native measure/placement runs every frame. Pixel rendering is sampled
                            // to avoid making the software emulator's bitmap draw the bottleneck.
                            if (frame % 16L == 0L) {
                                canvas.drawColor(android.graphics.Color.TRANSPARENT,
                                    android.graphics.PorterDuff.Mode.CLEAR)
                                root.draw(canvas)
                            }
                            stage.set("frame $frame done")
                        }
                        // Give asynchronous font/preparation work time to finish during warmup.
                        if (frame < 600) Thread.sleep(5)
                    }
                    check(completed.count == 0L) { "${file.name}: stress timed out after $frame frames" }
                } finally {
                    runOnMainSync {
                        root.disposeComposition()
                        recomposer.cancel()
                        scope.cancel()
                        LyricsSpringTrace.clear()
                    }
                    bitmap.recycle()
                }
                failure?.let { throw it }
                reports += report
            }
            finish(Activity.RESULT_OK, Bundle().apply {
                putString("stream", "${reports.joinToString("\n")}\nPASS ${files.size} songs, $totalToggles toggles\n")
            })
        } catch (error: Throwable) {
            Log.e(TAG, "Stress failed", error)
            finish(Activity.RESULT_CANCELED, Bundle().apply { putString("stream", "FAIL $error\n") })
        } finally {
            watchdog.interrupt()
        }
    }

    private companion object { const val TAG = "CaptionToggleStress" }
}
