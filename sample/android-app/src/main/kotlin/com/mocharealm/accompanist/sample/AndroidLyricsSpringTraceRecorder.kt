package com.mocharealm.accompanist.sample

import android.content.Context
import android.os.Process
import android.util.Log
import com.mocharealm.accompanist.lyrics.ui.diagnostics.LyricsSpringTrace
import com.mocharealm.accompanist.lyrics.ui.diagnostics.LyricsSpringTraceSink
import java.io.BufferedOutputStream
import java.io.ByteArrayOutputStream
import java.io.File
import java.io.FileOutputStream
import java.util.concurrent.ArrayBlockingQueue
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicLong
import java.util.zip.Deflater

/** Asynchronously writes independent, compressed binary records under the app's private files. */
internal object AndroidLyricsSpringTraceRecorder {
    private const val TAG = "LyricsSpringTrace"

    @Volatile private var session: Session? = null

    fun start(context: Context) {
        if (session != null) return
        val file = File(context.filesDir, "lyrics-spring.lstr")
        runCatching { file.delete() }
        val capture = Session(file)
        session = capture
        LyricsSpringTrace.install(capture.sink)
        capture.thread.start()
        Log.i(TAG, "Binary spring trace active at ${file.absolutePath}")
    }

    /** Optional explicit stop; complete compressed blocks remain readable after a process kill. */
    fun stop() {
        val capture = session ?: return
        LyricsSpringTrace.clear()
        capture.stopping.set(true)
        session = null
    }

    private class Session(private val file: File) {
        val queue = ArrayBlockingQueue<ByteArray>(4096)
        val stopping = AtomicBoolean(false)
        private val droppedRecords = AtomicLong(0)

        val sink = LyricsSpringTraceSink { record ->
            if (!queue.offer(record)) droppedRecords.incrementAndGet()
        }

        val thread = Thread(::writeLoop, "lyrics-spring-trace-writer")

        private fun writeLoop() {
            Process.setThreadPriority(Process.THREAD_PRIORITY_BACKGROUND)
            var written = 0L
            var lastFlushNanos = System.nanoTime()
            try {
                BufferedOutputStream(FileOutputStream(file, false), 64 * 1024).use { output ->
                    output.write(byteArrayOf('L'.code.toByte(), 'S'.code.toByte(), 'P'.code.toByte(), 'R'.code.toByte()))
                    writeIntLe(output, 1) // Container and record schema version.
                    output.flush()

                    while (!stopping.get() || queue.isNotEmpty()) {
                        val record = queue.poll(100, TimeUnit.MILLISECONDS)
                        if (record != null) {
                            val compressed = deflate(record)
                            writeIntLe(output, record.size)
                            writeIntLe(output, compressed.size)
                            output.write(compressed)
                            written++
                        }

                        val now = System.nanoTime()
                        if (written % 16L == 0L || now - lastFlushNanos >= 250_000_000L) {
                            output.flush()
                            lastFlushNanos = now
                        }
                    }
                    output.flush()
                }
            } catch (error: Throwable) {
                Log.e(TAG, "Could not write binary spring trace", error)
            } finally {
                val dropped = droppedRecords.get()
                Log.i(TAG, "Binary spring trace stopped; records=$written dropped=$dropped file=${file.absolutePath}")
            }
        }

        private fun deflate(record: ByteArray): ByteArray {
            val deflater = Deflater(Deflater.BEST_SPEED, true)
            try {
                deflater.setInput(record)
                deflater.finish()
                val compressed = ByteArrayOutputStream(record.size)
                val chunk = ByteArray(4096)
                while (!deflater.finished()) {
                    val count = deflater.deflate(chunk)
                    if (count == 0 && deflater.needsInput()) break
                    compressed.write(chunk, 0, count)
                }
                return compressed.toByteArray()
            } finally {
                deflater.end()
            }
        }

        private fun writeIntLe(output: BufferedOutputStream, value: Int) {
            output.write(value and 0xff)
            output.write((value ushr 8) and 0xff)
            output.write((value ushr 16) and 0xff)
            output.write((value ushr 24) and 0xff)
        }
    }
}
