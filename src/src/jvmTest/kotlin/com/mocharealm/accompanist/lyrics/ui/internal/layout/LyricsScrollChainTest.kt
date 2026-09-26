package com.mocharealm.accompanist.lyrics.ui.internal.layout

import com.mocharealm.accompanist.lyrics.ui.composable.list.*

import kotlin.test.*

class LyricsScrollChainTest {
    @Test
    fun focusLeadsAndFollowingItemsLagThenStop() {
        val chain = LyricsScrollChainState()
        chain.configure(10000, LyricsScrollChain(), 0.0, 600)
        chain.retain(0, 12)
        chain.focusAt(1)
        chain.moveTo(100.0, true)
        repeat(12) { chain.advance(1f / 120) }
        assertEquals(0f, chain.offset(0))
        assertEquals(0f, chain.offset(1))
        assertTrue(chain.offset(2) > 0f)
        assertTrue(chain.offset(3) > chain.offset(2))
        assertEquals(12, chain.retainedCount)
        repeat(2400) { chain.advance(1f / 120) }
        assertFalse(chain.active)
        for (i in 0..11) assertEquals(0f, chain.offset(i))
    }

    @Test
    fun frameRatesHaveConsistentResponse() {
        fun simulate(hz: Int): Float {
            val chain = LyricsScrollChainState()
            chain.configure(10, LyricsScrollChain(), 0.0, 600)
            chain.retain(0, 10)
            chain.focusAt(1)
            chain.moveTo(100.0, true)
            repeat(hz / 2) { chain.advance(1f / hz) }
            return chain.offset(4)
        }
        assertEquals(simulate(60), simulate(120), 0.1f)
    }

    @Test
    fun manualMovementResetAndNewlyRetainedItemsHaveNoStaleOffsets() {
        val chain = LyricsScrollChainState()
        chain.configure(100, LyricsScrollChain(), 0.0, 600)
        chain.retain(0, 10)
        chain.focusAt(1)
        chain.moveTo(300.0, true)
        chain.advance(0.02f)
        assertTrue(chain.active)
        chain.moveTo(320.0, false)
        assertFalse(chain.active)
        for (i in 0..9) assertEquals(0f, chain.offset(i))
        chain.retain(80, 90)
        for (i in 80..89) assertEquals(0f, chain.offset(i))
        chain.configure(100, null, 320.0, 600)
        chain.retain(80, 90)
        chain.moveTo(900.0, true)
        assertFalse(chain.active)
    }

    @Test
    fun rebasePreservesOffsetsAndPromotingFocusDoesNotSnap() {
        val chain = LyricsScrollChainState()
        chain.configure(10, LyricsScrollChain(), 0.0, 600)
        chain.retain(0, 10)
        chain.focusAt(1)
        chain.moveTo(100.0, true)
        chain.advance(0.02f)
        val before = chain.offset(2)
        chain.rebase(180.0)
        assertEquals(before, chain.offset(2))
        chain.focusAt(2)
        chain.moveTo(181.0, true)
        assertEquals(before, chain.offset(2), 0.001f)
        repeat(2400) { chain.advance(1f / 120) }
        assertFalse(chain.active)
    }
}
