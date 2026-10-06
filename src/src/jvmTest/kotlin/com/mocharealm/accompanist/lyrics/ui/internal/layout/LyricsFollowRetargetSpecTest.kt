package com.mocharealm.accompanist.lyrics.ui.internal.layout

import androidx.compose.animation.core.*
import kotlin.math.abs
import kotlin.test.*

class LyricsFollowRetargetSpecTest {
    @Test
    fun retargetKeepsConfiguredDurationAndIncomingVelocity() {
        for (duration in listOf(650, 10_000)) for (easing in listOf(LinearEasing, FastOutSlowInEasing)) {
            val spec = LyricsFollowRetargetSpec(tween(duration, easing = easing))
            for (velocity in listOf(-120f, 0f, 80f, 500f)) {
                assertEquals(duration * 1_000_000L, spec.getDurationNanos(0f, 200f, velocity))
                assertEquals(0f, spec.getValueFromNanos(0L, 0f, 200f, velocity))
                assertEquals(velocity, spec.getVelocityFromNanos(0L, 0f, 200f, velocity))
                val initialDerivative = spec.getValueFromNanos(10_000L, 0f, 200f, velocity) / 0.00001f
                assertEquals(velocity, initialDerivative, 2f, "Actual position derivative must match the carried velocity")
                assertEquals(200f, spec.getValueFromNanos(duration * 1_000_000L, 0f, 200f, velocity))
            }
        }
    }

    @Test
    fun dragResumeBoundaryDoesNotSwitchToAHighStiffnessScroll() {
        // The local Sugar Talking/The Code reproduction moves about 200px at a carried 57px/s.
        val native = tween<Float>(650, easing = FastOutSlowInEasing).vectorize(Float.VectorConverter)
        val spec = LyricsFollowRetargetSpec(tween(650, easing = FastOutSlowInEasing))
        val before = 57f
        val at16ms = spec.getVelocityFromNanos(16_000_000L, 0f, 200f, before)
        val at32ms = spec.getVelocityFromNanos(32_000_000L, 0f, 200f, before)
        assertTrue(abs(at16ms - before) < 100f, "16ms handoff must not inject thousands of pixels/second")
        assertTrue(abs(at32ms - at16ms) < 100f)
        val start = AnimationVector1D(0f)
        val target = AnimationVector1D(200f)
        val velocity = AnimationVector1D(before)
        val nativePeak = (1..650).maxOf { abs(native.getVelocityFromNanos(it * 1_000_000L, start, target, velocity).value) }
        val peak = (1..650).maxOf { abs(spec.getVelocityFromNanos(it * 1_000_000L, 0f, 200f, before)) }
        assertTrue(peak <= nativePeak + before + 1f, "Retarget stays within the configured curve plus carried momentum")
    }

    @Test
    fun callerSpringRemainsTheSameSpring() {
        val spec = spring<Float>(stiffness = 75f, dampingRatio = 0.8f)
        assertSame(spec, lyricsFollowRetargetSpec(spec))
    }
}
