package com.mocharealm.accompanist.lyrics.ui.internal.diagnostics

import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.atomic.AtomicLong
import java.util.concurrent.atomic.LongAdder

private val enabled by lazy { java.lang.Boolean.getBoolean("accompanist.frameTiming") }
private data class TraceSpan(val name: String, val started: Long)
private class TraceTotals {
    val calls = LongAdder()
    val nanos = LongAdder()
    val maximum = AtomicLong()
}
private val spans = ThreadLocal.withInitial { ArrayDeque<TraceSpan>() }
private val totals = ConcurrentHashMap<String, TraceTotals>()
private val lastReport = AtomicLong(System.nanoTime())

internal actual fun lyricsTraceEnabled(): Boolean = enabled

internal actual fun setLyricsTraceCounter(name: String, value: Long) = Unit

internal actual fun beginLyricsTrace(name: String) {
    spans.get().addLast(TraceSpan(name, System.nanoTime()))
}

internal actual fun endLyricsTrace() {
    val span = spans.get().removeLastOrNull() ?: return
    val elapsed = System.nanoTime() - span.started
    val total = totals.computeIfAbsent(span.name) { TraceTotals() }
    total.calls.increment()
    total.nanos.add(elapsed)
    total.maximum.accumulateAndGet(elapsed, ::maxOf)
    val now = System.nanoTime()
    val previous = lastReport.get()
    if (now - previous >= 1_000_000_000L && lastReport.compareAndSet(previous, now)) {
        for ((name, values) in totals.entries.sortedBy { it.key }) {
            val calls = values.calls.sumThenReset()
            val nanos = values.nanos.sumThenReset()
            val maximum = values.maximum.getAndSet(0)
            if (calls > 0) System.err.println("[lyrics-perf] $name calls=$calls " +
                "total=${nanos / 1_000_000.0}ms avg=${nanos / calls / 1_000_000.0}ms " +
                "max=${maximum / 1_000_000.0}ms")
        }
    }
}
