package com.mocharealm.accompanist.lyrics.ui.internal.test

import java.util.concurrent.ConcurrentLinkedQueue
import kotlin.coroutines.CoroutineContext
import kotlinx.coroutines.CoroutineDispatcher

/** Keeps scene effects on the thread that measures and draws, including worker resumptions. */
internal class TestSceneDispatcher : CoroutineDispatcher() {
    private val owner = Thread.currentThread()
    private val tasks = ConcurrentLinkedQueue<Runnable>()

    override fun isDispatchNeeded(context: CoroutineContext): Boolean =
        Thread.currentThread() !== owner

    override fun dispatch(context: CoroutineContext, block: Runnable) {
        tasks.add(block)
    }

    fun runCurrent() {
        check(Thread.currentThread() === owner)
        while (true) (tasks.poll() ?: return).run()
    }
}
