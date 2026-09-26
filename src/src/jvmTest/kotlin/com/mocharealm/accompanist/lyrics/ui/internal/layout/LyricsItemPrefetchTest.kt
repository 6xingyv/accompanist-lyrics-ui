package com.mocharealm.accompanist.lyrics.ui.internal.layout

import com.mocharealm.accompanist.lyrics.ui.composable.list.*

import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.lazy.layout.LazyLayoutPrefetchState
import androidx.compose.ui.unit.Constraints
import kotlin.test.*

@OptIn(ExperimentalFoundationApi::class)
class LyricsItemPrefetchTest {
    @Test
    fun fastScrollExpandsHorizonAndKeepsOverlappingRequests() {
        var now = 0L
        val requested = mutableListOf<Int>()
        val cancelled = mutableListOf<Int>()
        val prefetch =
            LyricsItemPrefetch(LazyLayoutPrefetchState(), { now }) { index, _ ->
                requested.add(index)
                val cancel: () -> Unit = { cancelled.add(index) }
                cancel
            }
        val constraints = Constraints(maxWidth = 400)
        prefetch.measured(10, 18, 100, constraints, 100f, 600f)
        now += 16_000_000
        prefetch.onScroll(100f)
        assertEquals(listOf(18, 19, 20), requested)
        prefetch.measured(11, 19, 100, constraints, 100f, 600f)
        assertEquals(listOf(18), cancelled)
        assertEquals(listOf(18, 19, 20, 21), requested)
        now += 16_000_000
        prefetch.onScroll(-1f)
        assertEquals(10, requested.last())
        assertEquals(4, cancelled.size)
        now += 1_000_000_000
        prefetch.onScroll(-1f)
        assertEquals(5, requested.size)
    }

    @Test
    fun requestsStayBoundedAndCancelOnDirectionConstraintsAndDisposal() {
        val requested = mutableListOf<Int>()
        val cancelled = mutableListOf<Int>()
        val prefetch =
            LyricsItemPrefetch(LazyLayoutPrefetchState()) { index, _ ->
                requested.add(index)
                val cancel: () -> Unit = {
                    cancelled.add(index)
                    Unit
                }
                cancel
            }
        val constraints = Constraints(maxWidth = 400)
        prefetch.measured(10, 18, 100, constraints)
        repeat(10) { prefetch.onScroll(1f) }
        assertEquals(listOf(18), requested)
        prefetch.onScroll(-1f)
        assertEquals(listOf(18), cancelled)
        assertEquals(listOf(18, 9), requested)
        prefetch.measured(10, 18, 100, Constraints(maxWidth = 500))
        assertEquals(listOf(18, 9), cancelled)
        prefetch.cancel()
        assertEquals(listOf(18, 9, 9), cancelled)
        prefetch.measured(0, 0, 0, constraints)
        assertEquals(3, requested.size)
    }
}
