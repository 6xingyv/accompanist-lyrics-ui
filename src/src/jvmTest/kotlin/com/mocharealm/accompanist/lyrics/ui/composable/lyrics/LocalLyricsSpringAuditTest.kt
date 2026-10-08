package com.mocharealm.accompanist.lyrics.ui.composable.lyrics

import com.mocharealm.accompanist.lyrics.ui.internal.test.TestSceneDispatcher

import androidx.compose.foundation.interaction.DragInteraction
import androidx.compose.animation.core.FastOutSlowInEasing
import androidx.compose.animation.core.FloatTweenSpec
import androidx.compose.runtime.*
import androidx.compose.runtime.snapshots.Snapshot
import androidx.compose.ui.InternalComposeUiApi
import androidx.compose.ui.graphics.Canvas
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.platform.FrameRecomposer
import androidx.compose.ui.scene.CanvasLayersComposeScene
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.unit.IntSize
import com.ibm.icu.text.Transliterator
import com.mocharealm.accompanist.lyrics.core.model.ISyncedLine
import com.mocharealm.accompanist.lyrics.core.model.karaoke.*
import com.mocharealm.accompanist.lyrics.core.model.synced.SyncedLine
import com.mocharealm.accompanist.lyrics.core.parser.TTMLParser
import com.mocharealm.accompanist.lyrics.core.utils.PhoneticProvider
import com.mocharealm.accompanist.lyrics.ui.composable.list.LyricsLazyListState
import com.mocharealm.accompanist.lyrics.ui.diagnostics.LyricsSpringTrace
import java.io.BufferedOutputStream
import java.nio.file.Files
import java.nio.file.Path
import java.nio.file.StandardOpenOption
import java.nio.ByteBuffer
import java.nio.ByteOrder
import java.util.zip.Deflater
import kotlinx.coroutines.*
import kotlin.math.abs
import kotlin.test.*

/** Optional local corpus audit; portable spring regression tests live alongside this harness. */
@OptIn(InternalComposeUiApi::class)
class LocalLyricsSpringAuditTest {
    @Test
    fun localSongsExerciseClicksDragVocalsAndInterludes() {
        val root = Path.of("C:/Users/Simon/Music/Local")
        if (!Files.isDirectory(root)) return
        val filter = System.getenv("LYRICS_AUDIT_FILTER")
        val drawFrames = System.getenv("LYRICS_AUDIT_LAYOUT_ONLY") != "1"
        val playthrough = System.getenv("LYRICS_AUDIT_PLAYTHROUGH") == "1"
        val toggleCaptions = System.getenv("LYRICS_AUDIT_TOGGLE") == "1"
        val allSongs = playthrough || System.getenv("LYRICS_AUDIT_ALL") == "1"
        val wanted = listOf("Leave the Door Open", "漫步人生路", "Sugar Talking", "群青", "水星记", "The Code", "En tu piel")
        val files = Files.walk(root).use { paths ->
            paths.filter { Files.isRegularFile(it) && it.toString().endsWith(".ttml", true) &&
                (allSongs || wanted.any { title -> it.fileName.toString().contains(title, true) }) &&
                (filter == null || it.fileName.toString().contains(filter, true)) }
                .sorted().toList()
        }
        assertTrue(files.isNotEmpty(), "The local lyrics corpus must supply audit songs")
        val output = Path.of(when {
            toggleCaptions -> "build/reports/spring-audit-toggle-late"
            playthrough -> "build/reports/spring-audit-playthrough"
            allSongs -> "build/reports/spring-audit-all"
            else -> "build/reports/spring-audit"
        })
        Files.createDirectories(output)
        val results = output.resolve("results.tsv")
        val previousResults = if (allSongs && System.getenv("LYRICS_AUDIT_RESUME") == "1" && Files.exists(results)) {
            Files.readAllLines(results).drop(1).associateBy { it.substringBefore('\t').toInt() }
        } else emptyMap()
        Files.writeString(results, "index\tstatus\tmainLines\tframes\tactorFrames\tretargets\tmaxActorSpeed\tmaxRowSpeed\tmaxRowAcceleration\tsource\tdetail\n")
        val failures = mutableListOf<String>()
        fun result(index: Int, status: String, file: Path, detail: String, metrics: String = "0\t0\t0\t0\t0\t0\t0") {
            Files.writeString(results, "$index\t$status\t$metrics\t$file\t${detail.replace('\n', ' ').replace('\r', ' ').replace('\t', ' ')}\n", StandardOpenOption.APPEND)
        }
        val transliterator = Transliterator.getInstance("Any-Latin; Latin-ASCII")
        val parser = TTMLParser(object : PhoneticProvider {
            override val phoneticLevel = PhoneticLevel.SYLLABLE
            override fun getPhonetic(string: String) = transliterator.transliterate(string).lowercase()
        })
        fun hasVocals(line: ISyncedLine) = line is KaraokeLine.MainKaraokeLine && !line.accompanimentLines.isNullOrEmpty()
        for ((songIndex, file) in files.withIndex()) {
            val previous = previousResults[songIndex]
            if (previous != null && previous.split('\t').let { it[1] == "PASS" && it[9] == file.toString() }) {
                Files.writeString(results, "$previous\n", StandardOpenOption.APPEND)
                continue
            }
            println("AUDIT ${songIndex + 1}/${files.size}: $file")
            try {
                val lyrics = parser.parse(Files.readString(file))
                val main = lyrics.lines.filter { it is KaraokeLine.MainKaraokeLine || it is SyncedLine }
                if (main.isEmpty()) {
                    result(songIndex, "NO_MAIN_LINES", file, "Parsed successfully; no timed primary lines to animate")
                    continue
                }
                val state = LyricsLazyListState()
                val time = mutableIntStateOf(if (playthrough) 0 else main.first().start)
                var phoneticShown by mutableStateOf(true)
                var translationShown by mutableStateOf(true)
                val dispatcher = TestSceneDispatcher()
                val recomposer = FrameRecomposer(dispatcher)
                val scene = CanvasLayersComposeScene(recomposer, size = IntSize(400, 900))
                val canvas = Canvas(ImageBitmap(400, 900))
                lateinit var scope: CoroutineScope
                var millis = 0L
                var phase = 0
                var maxScrollSpeed = 0.0
                var maxScrollSpeedStep = 0.0
                var previousPosition = 0.0
                var previousSpeed = 0.0
                var auditFrame: () -> Unit = {}
                val trace = BinaryTrace(output.resolve("song-$songIndex.lstr"))
                fun frame(play: Boolean = false) {
                    if (play) time.intValue += 16
                    millis += 16
                    state.chain.traceMarker(200, eventIndex = phase, eventValue = time.intValue.toDouble(), eventValue2 = millis.toDouble())
                    dispatcher.runCurrent()
                    Snapshot.sendApplyNotifications()
                    recomposer.performFrame(millis * 1_000_000)
                    scene.measureAndLayout()
                    // Layout and frame callbacks run the real scroll actor and row springs. The full
                    // corpus needs no pixel readback; the smaller visual audit still draws every frame.
                    if (!allSongs && drawFrames) scene.draw(canvas)
                    if (playthrough || toggleCaptions) state.chain.traceMarker(202, eventIndex = phase,
                        eventValue = time.intValue.toDouble(), eventValue2 = millis.toDouble())
                    val speed = (state.position - previousPosition) / 0.016
                    maxScrollSpeed = maxOf(maxScrollSpeed, abs(speed))
                    maxScrollSpeedStep = maxOf(maxScrollSpeedStep, abs(speed - previousSpeed))
                    assertTrue(state.position.isFinite() && speed.isFinite(), "${file.fileName}: non-finite scrolling")
                    auditFrame()
                    previousPosition = state.position
                    previousSpeed = speed
                }
                fun phase(value: Int) {
                    phase = value
                    state.chain.traceMarker(201, eventIndex = phase, eventValue = time.intValue.toDouble(), eventValue2 = millis.toDouble())
                }
                fun indexOf(line: ISyncedLine): Int = state.items.indexOfFirst {
                    val source = it.key.toString().substringAfterLast('-').toIntOrNull()
                    source != null && lyrics.lines[source] == line
                }.also { assertTrue(it >= 0, "Main line must map to a displayed item") }
                fun click(line: ISyncedLine) {
                    state.resumeAutoScroll(line.start, indexOf(line))
                    time.intValue = line.start
                }
                var boundarySpeedBound: Float? = null
                var auditedBoundaryFrames = 0
                var coordinateShift = 0f
                var actorFrames = 0
                var retargets = 0
                var maxActorSpeed = 0f
                var maxRowSpeed = 0f
                var maxRowAcceleration = 0f
                var lastFocus = 0
                var lastActorVelocity = 0f
                val focusHandoffs = mutableListOf<String>()
                val speedViolations = mutableListOf<String>()
                val previousRowVelocities = mutableMapOf<Int, Float>()
                var previousRetargets = 0
                auditFrame = {
                    if (playthrough && phase == 30 && retargets > previousRetargets &&
                        abs(lastActorVelocity) > 80f && abs(state.position - previousPosition) < 0.01 &&
                        speedViolations.size < 20) {
                        speedViolations += "retarget stalled one frame: time=${time.intValue} reportedVelocity=$lastActorVelocity"
                    }
                    previousRetargets = retargets
                }
                LyricsSpringTrace.install { record ->
                    trace.write(record)
                    val data = ByteBuffer.wrap(record).order(ByteOrder.LITTLE_ENDIAN)
                    val code = data.getInt(21)
                    if (code == 111) coordinateShift = data.getDouble(37).toFloat()
                    if (allSongs || toggleCaptions || (main.size >= 3 && phase == 2 &&
                        time.intValue in (main[1].end - 100)..(main[1].end + 1000) && data.getInt(25) == indexOf(main[2]))) {
                        if (code == 112) {
                            val from = (data.getDouble(49) - coordinateShift).toFloat()
                            val to = data.getDouble(29).toFloat()
                            val initialVelocity = data.getDouble(37).toFloat()
                            val native = FloatTweenSpec(650, easing = FastOutSlowInEasing)
                            val nativePeak = (1..650).maxOf { abs(native.getVelocityFromNanos(it * 1_000_000L, from, to, initialVelocity)) }
                            val nativeStart = native.getVelocityFromNanos(1_000_000L, from, to, initialVelocity)
                            boundarySpeedBound = maxOf(abs(initialVelocity), nativePeak + abs(initialVelocity - nativeStart)) + 10f
                        } else if (code == 120 && boundarySpeedBound != null) {
                            val velocity = data.getFloat(45)
                            if (!velocity.isFinite() || abs(velocity) > boundarySpeedBound) {
                                if (speedViolations.size < 20) speedViolations +=
                                    "phase=$phase time=${time.intValue} row=${data.getInt(25)} speed=$velocity bound=$boundarySpeedBound"
                            }
                            if (phase == 2) auditedBoundaryFrames++
                        }
                    }
                    if (code == 112 && data.getFloat(45) != 0f) retargets++
                    if (code == 120) {
                        actorFrames++
                        lastActorVelocity = data.getFloat(45)
                        maxActorSpeed = maxOf(maxActorSpeed, abs(lastActorVelocity))
                    }
                    if (playthrough && phase == 30 && code == 105 && abs(lastActorVelocity) > 80f) {
                        val nextFocus = data.getInt(61)
                        if (nextFocus != lastFocus) focusHandoffs +=
                            "${time.intValue}\t$lastFocus\t$nextFocus\t$lastActorVelocity"
                    }
                    val nextFocus = data.getInt(61)
                    repeat(data.getInt(94)) { row ->
                        val offset = 98 + row * 48
                        val index = data.getInt(offset)
                        val velocity = data.getFloat(offset + 20)
                        val acceleration = data.getFloat(offset + 36)
                        val previousVelocity = previousRowVelocities[index]
                        if (playthrough && phase == 30 && code == 105 && previousVelocity != null) {
                            val before = previousVelocity + if (index <= lastFocus) lastActorVelocity else 0f
                            val after = velocity + if (index <= nextFocus) lastActorVelocity else 0f
                            if (abs(before - after) > 1f && speedViolations.size < 20) {
                                speedViolations += "focus changed screen velocity: time=${time.intValue} row=$index before=$before after=$after"
                            }
                        }
                        previousRowVelocities[index] = velocity
                        assertTrue(velocity.isFinite() && acceleration.isFinite(), "Non-finite row spring at phase=$phase row=${data.getInt(offset)}")
                        maxRowSpeed = maxOf(maxRowSpeed, abs(velocity))
                        maxRowAcceleration = maxOf(maxRowAcceleration, abs(acceleration))
                    }
                    lastFocus = nextFocus
                }
                try {
                    scene.setContent {
                        scope = rememberCoroutineScope()
                        KaraokeLyricsView(state, lyrics, { time.intValue }, {}, {},
                            translationTextStyle = TextStyle(), showPhonetic = phoneticShown,
                            showTranslation = translationShown,
                            autoScrollResumeDelayMillis = 0)
                    }
                    val deadline = System.nanoTime() + 10_000_000_000L
                    while (!state.ready && System.nanoTime() < deadline) { frame(); Thread.sleep(3) }
                    assertTrue(state.ready)
                    repeat(100) { frame() }
                    if (toggleCaptions && main.size >= 2) {
                        val checks = StringBuilder("source=$file\n")
                        val positions = listOf(1, main.size / 3, main.size / 2, main.lastIndex - 1)
                            .map { it.coerceIn(1, main.lastIndex) }.distinct()
                        for (position in positions) for (translation in listOf(false, true)) {
                            phoneticShown = true
                            translationShown = true
                            repeat(100) { frame() }
                            click(main[position])
                            repeat(150) { frame() }
                            time.intValue = main[position].end - 240
                            repeat(100) { frame() }
                            phase(40)
                            val anchorIndex = state.followAnchorIndex
                            fun screenTop(index: Int) = state.heights.top(index) - state.position + state.chain.offset(index)
                            val before = screenTop(anchorIndex)
                            var maximumDrift = 0.0
                            repeat(120) { index ->
                                if (index % 2 == 0) {
                                    if (translation) translationShown = !translationShown else phoneticShown = !phoneticShown
                                }
                                frame()
                                maximumDrift = maxOf(maximumDrift, abs(screenTop(anchorIndex) - before))
                            }
                            checks.append("position=$position translation=$translation anchor=$anchorIndex maximumDrift=$maximumDrift\n")
                            assertTrue(maximumDrift < 2.0,
                                "Caption toggles must preserve the current anchor; position=$position translation=$translation drift=$maximumDrift")
                            phase(41)
                            repeat(320) { index ->
                                if (index % 2 == 0) {
                                    if (translation) translationShown = !translationShown else phoneticShown = !phoneticShown
                                }
                                frame(true)
                            }
                            phoneticShown = false
                            translationShown = false
                            repeat(150) { frame() }
                            val finalTop = screenTop(state.followAnchorIndex)
                            checks.append("afterHandoff=${state.followAnchorIndex} screenTop=$finalTop\n")
                            Files.writeString(output.resolve("song-$songIndex.caption-drift.txt"), checks.toString())
                            assertTrue(abs(finalTop) < 2.0,
                                "Focused row must stay at the anchor after reflow; position=$position translation=$translation top=$finalTop")
                        }
                    } else if (playthrough) {
                        phase(30)
                        val endTime = lyrics.lines.maxOf { it.end }.toLong() + 2000L
                        while (time.intValue.toLong() <= endTime) frame(true)
                    } else {
                        phase(1) // Rapid clicks while the previous scroll and row springs are running.
                        for (line in main.drop(1).take(5)) {
                            click(line)
                            repeat(8) { frame(true) }
                            click(line)
                            repeat(3) { frame(true) }
                        }
                        repeat(100) { frame() }
                        if (main.size >= 2) {
                            phase(2) // Click the second row, drag it down, then let its real boundary pass.
                            click(main[1])
                            repeat(100) { frame() }
                            time.intValue = main[1].end - 400
                            repeat(70) { frame() }
                            val drag = DragInteraction.Start()
                            scope.launch { state.interactionSource.emit(drag) }
                            repeat(3) { frame() }
                            state.dispatchRawDelta(-40f)
                            repeat(5) { frame() }
                            scope.launch { state.interactionSource.emit(DragInteraction.Stop(drag)) }
                            repeat(100) { frame(true) }
                        }
                        // Actual backing-vocal and long-gap transitions, using the source timestamps.
                        val boundaries = buildList {
                            addAll(main.filter(::hasVocals).take(2).map { it.end })
                            addAll(main.zipWithNext().filter { (a, b) -> b.start - a.end > 5000 }.take(2).flatMap { (a, b) -> listOf(a.end, b.start) })
                        }.distinct()
                        for ((index, boundary) in boundaries.withIndex()) {
                            phase(10 + index)
                            time.intValue = (boundary - 400).coerceAtLeast(0)
                            repeat(100) { frame() }
                            repeat(95) { frame(true) }
                        }
                    }
                    println("${file.fileName}: main=${main.size} vocals=${main.count(::hasVocals)} boundaryFrames=$auditedBoundaryFrames peakScrollSpeed=$maxScrollSpeed maxSpeedStep=$maxScrollSpeedStep frames=${millis / 16}")
                    Files.writeString(output.resolve("song-$songIndex.txt"), "source=$file\nframes=${millis / 16}\npeakScrollSpeed=$maxScrollSpeed\nmaxSpeedStep=$maxScrollSpeedStep\n")
                    if (playthrough) Files.writeString(output.resolve("song-$songIndex.focus-handoffs.tsv"),
                        "timeMs\tpreviousFocus\tnextFocus\tscrollVelocity\n" + focusHandoffs.joinToString("\n"))
                    assertTrue(speedViolations.isEmpty(), speedViolations.joinToString("; "))
                    Files.deleteIfExists(output.resolve("song-$songIndex.failure.txt"))
                    result(songIndex, "PASS", file, "vocals=${main.count(::hasVocals)} dragActorFrames=$auditedBoundaryFrames draw=${!allSongs && drawFrames} playthrough=$playthrough endTime=${time.intValue} movingFocusHandoffs=${focusHandoffs.size}",
                        "${main.size}\t${millis / 16}\t$actorFrames\t$retargets\t$maxActorSpeed\t$maxRowSpeed\t$maxRowAcceleration")
                } finally {
                    LyricsSpringTrace.clear()
                    trace.close()
                    scene.close()
                    recomposer.close()
                    dispatcher.runCurrent()
                }
            } catch (failure: Throwable) {
                if (failure is VirtualMachineError) throw failure
                val detail = "${failure::class.simpleName}: ${failure.message}"
                failures += "$file: $detail"
                result(songIndex, "FAIL", file, detail)
                Files.writeString(output.resolve("song-$songIndex.failure.txt"), failure.stackTraceToString())
            }
        }
        println("AUDIT COMPLETE: files=${files.size} failures=${failures.size} results=$results")
        assertTrue(failures.isEmpty(), "${failures.size}/${files.size} lyrics audits failed:\n${failures.joinToString("\n")}")
    }

    private class BinaryTrace(file: Path) : AutoCloseable {
        private val output = BufferedOutputStream(Files.newOutputStream(file))
        private val deflater = Deflater(Deflater.BEST_SPEED, true)
        private val compressed = java.io.ByteArrayOutputStream()
        private val buffer = ByteArray(4096)
        init { output.write(byteArrayOf(76, 83, 80, 82)); integer(1) }
        private fun integer(value: Int) { repeat(4) { output.write((value ushr (it * 8)) and 255) } }
        fun write(record: ByteArray) {
            deflater.reset()
            compressed.reset()
            deflater.setInput(record)
            deflater.finish()
            while (!deflater.finished()) compressed.write(buffer, 0, deflater.deflate(buffer))
            integer(record.size)
            integer(compressed.size())
            compressed.writeTo(output)
        }
        override fun close() { try { output.close() } finally { deflater.end() } }
    }
}
