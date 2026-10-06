package com.mocharealm.accompanist.lyrics.ui.internal.layout

import com.mocharealm.accompanist.lyrics.ui.composable.list.*
import com.mocharealm.accompanist.lyrics.ui.diagnostics.LyricsSpringTrace
import java.nio.ByteBuffer
import java.nio.ByteOrder

import kotlin.test.*

class LyricsScrollChainTest {
    @Test
    fun focusHandoffsPreserveScreenVelocityInBothDirections() {
        val chain = LyricsScrollChainState()
        chain.configure(10, LyricsScrollChain(), 0.0, 900)
        chain.retain(0, 10)
        chain.focusAt(1)
        chain.followScrollTo(100.0)
        chain.advance(0.016f)
        val velocities = FloatArray(10)
        LyricsSpringTrace.install { record ->
            val data = ByteBuffer.wrap(record).order(ByteOrder.LITTLE_ENDIAN)
            repeat(data.getInt(94)) { row ->
                val offset = 98 + row * 48
                velocities[data.getInt(offset)] = data.getFloat(offset + 20)
            }
        }
        try {
            chain.traceMarker(299)
            val before = velocities.copyOf()
            val offset = chain.offset(2)
            // Natural twilight zone playback promotes a moving row; clicks can scroll backwards.
            for (scrollVelocity in listOf(610f, -610f)) {
                chain.focusAt(2, scrollVelocity)
                assertEquals(before[2], velocities[2] + scrollVelocity, 0.001f,
                    "Promotion must preserve the row's screen velocity, not add scroll velocity twice")
                assertEquals(offset, chain.offset(2))
                assertEquals(before[1], velocities[1])
                assertEquals(before[3], velocities[3])
                chain.focusAt(1, scrollVelocity)
                assertEquals(before[2], velocities[2], 0.001f,
                    "Demotion must restore the absolute velocity basis")
                assertEquals(offset, chain.offset(2))
            }
        } finally { LyricsSpringTrace.clear() }
    }

    @Test
    fun focusLeadsAndFollowingItemsLagThenStop() {
        val chain = LyricsScrollChainState()
        chain.configure(10000, LyricsScrollChain(), 0.0, 600)
        chain.retain(0, 12)
        chain.focusAt(1)
        chain.followScrollTo(100.0)
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
            chain.followScrollTo(100.0)
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
        chain.followScrollTo(300.0)
        chain.advance(0.02f)
        assertTrue(chain.active)
        chain.reset(300.0)
        chain.rebase(320.0)
        assertFalse(chain.active)
        for (i in 0..9) assertEquals(0f, chain.offset(i))
        chain.retain(80, 90)
        for (i in 80..89) assertEquals(0f, chain.offset(i))
        chain.configure(100, null, 320.0, 600)
        chain.retain(80, 90)
        chain.followScrollTo(900.0)
        assertFalse(chain.active)
    }

    @Test
    fun rebasePreservesOffsetsAndPromotingFocusDoesNotSnap() {
        val chain = LyricsScrollChainState()
        chain.configure(10, LyricsScrollChain(), 0.0, 600)
        chain.retain(0, 10)
        chain.focusAt(1)
        chain.followScrollTo(100.0)
        chain.advance(0.02f)
        val before = chain.offset(2)
        chain.rebase(180.0)
        assertEquals(before, chain.offset(2))
        chain.focusAt(2)
        chain.followScrollTo(181.0)
        assertEquals(before, chain.offset(2), 0.001f)
        repeat(2400) { chain.advance(1f / 120) }
        assertFalse(chain.active)
    }

    @Test
    fun changingScrollPathAndFocusKeepsSpringState() {
        val chain = LyricsScrollChainState()
        chain.configure(20, LyricsScrollChain(), 0.0, 600)
        chain.retain(0, 12)
        chain.focusAt(1)
        chain.followScrollTo(120.0)
        chain.advance(0.025f)
        val beforeRebase = chain.offset(5)
        assertTrue(beforeRebase > 0f)
        assertTrue(chain.active)

        // A list-coordinate update and another follow target change belong to different paths.
        // Neither is allowed to recreate the row springs or discard their displacement.
        chain.rebase(160.0)
        chain.focusAt(2)
        assertEquals(beforeRebase, chain.offset(5), 0.001f)
        chain.followScrollTo(161.0)
        chain.advance(0.025f)
        assertTrue(chain.active)
        assertTrue(chain.offset(5) > 0f)
    }
}
