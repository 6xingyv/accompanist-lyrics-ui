package com.mocharealm.accompanist.lyrics.ui.composable.lyrics

import androidx.compose.animation.core.FastOutSlowInEasing
import androidx.compose.animation.core.LinearEasing
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.BlendMode
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Paint
import androidx.compose.ui.graphics.drawscope.drawIntoCanvas
import androidx.compose.ui.graphics.drawscope.scale
import androidx.compose.ui.graphics.drawscope.withTransform
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import com.mocharealm.accompanist.lyrics.core.model.karaoke.KaraokeAlignment
import kotlin.math.PI
import kotlin.math.cos
import kotlin.math.sin

data class KaraokeBreathingDotsDefaults(
    val number: Int = 3,
    val size: Dp = 16.dp,
    val margin: Dp = 8.dp,
    val enterDurationMs: Int = 3000,
    val preExitStillDuration: Int = 200,
    val preExitDipAndRiseDuration: Int = 3000,
    val exitDurationMs: Int = 200,
    val breathingDotsColor: Color = Color.White
)

/**
 * Displays breathing dots animation during instrumental intros or interludes.
 * The dots breathe/pulse and fade in/out to indicate progress during non-lyrical sections.
 *
 * @param alignment Alignment of the dots (Start or End).
 * @param startTimeMs Start time of the interlude.
 * @param endTimeMs End time of the interlude.
 * @param currentTimeProvider Provider for current playback time.
 * @param modifier Modifier for the layout.
 * @param defaults Configuration defaults for size, count, etc.
 */
@Composable
fun KaraokeBreathingDots(
    alignment: KaraokeAlignment,
    startTimeMs: Int,
    endTimeMs: Int,
    currentTimeProvider: () -> Int,
    modifier: Modifier = Modifier,
    defaults: KaraokeBreathingDotsDefaults = KaraokeBreathingDotsDefaults(),
) {
    Box(modifier) {
        val size = with(LocalDensity.current) { defaults.size.toPx() }
        val margin = with(LocalDensity.current) { defaults.margin.toPx() }
        val totalWidth = size * defaults.number + margin * (defaults.number - 1)

        Canvas(
            Modifier
                .align(
                    when (alignment) {
                        KaraokeAlignment.Start -> Alignment.TopStart
                        KaraokeAlignment.End -> Alignment.TopEnd
                        else -> Alignment.TopStart
                    }
                )
                .padding(vertical = 8.dp, horizontal = 16.dp)
                .size(
                    width = defaults.size * defaults.number + defaults.margin * (defaults.number - 1),
                    height = defaults.size
                )
        ) {
            if (totalWidth <= 0f) return@Canvas

            val currentTime = currentTimeProvider().toFloat()
            val breathingAmplitude = 0.1f // Amplitude (0.8 to 1.0)
            val breathingCenter = 0.9f  // Center
            val breathingTrough = breathingCenter - breathingAmplitude

            val enterDuration = defaults.enterDurationMs.toFloat()
            val exitDuration = defaults.exitDurationMs.toFloat()
            val preExitStillDuration = defaults.preExitStillDuration.toFloat()
            val preExitDipAndRiseDuration = defaults.preExitDipAndRiseDuration.toFloat()

            val exitStartTime = endTimeMs - exitDuration
            val preExitStillStartTime = exitStartTime - preExitStillDuration
            val preExitDipAndRiseStartTime = preExitStillStartTime - preExitDipAndRiseDuration
            val breathingStartTime = startTimeMs + enterDuration
            val breathingDuration = preExitDipAndRiseStartTime - breathingStartTime

            var scale: Float
            var alpha: Float
            var revealProgress: Float

            if (breathingDuration < 0) {
                val overallProgress = when {
                    currentTime < startTimeMs + enterDuration -> (currentTime - startTimeMs) / enterDuration
                    currentTime > endTimeMs - exitDuration -> (endTimeMs - currentTime) / exitDuration
                    else -> 1f
                }.coerceIn(0f, 1f)

                scale = overallProgress
                alpha = overallProgress
                revealProgress = if (currentTime < startTimeMs + enterDuration) overallProgress else 1f
            } else {
                when {
                    // 1. Enter phase (0.0 -> 0.8)
                    currentTime < breathingStartTime -> {
                        val progress = (currentTime - startTimeMs) / enterDuration
                        alpha = FastOutSlowInEasing.transform(progress)
                        scale = alpha * 0.8f
                        revealProgress = alpha
                    }

                    // 2. Breathing phase (0.8 <-> 1.0)
                    currentTime < preExitDipAndRiseStartTime -> {
                        alpha = 1f
                        revealProgress = 1f
                        val timeInPhase = currentTime - breathingStartTime
                        val angle = (timeInPhase / 3000f) * 2 * PI
                        scale = 0.9f - 0.1f * cos(angle.toFloat())
                    }

                    // 3. Pre-exit
                    currentTime < preExitStillStartTime -> {
                        alpha = 1f
                        revealProgress = 1f
                        val phaseProgress = (currentTime - preExitDipAndRiseStartTime) / preExitDipAndRiseDuration

                        val angle = phaseProgress * 2 * PI
                        val cosValue = cos(angle.toFloat()) // 1 -> -1 -> 1

                        scale = 0.8f + 0.2f * cosValue
                    }

                    // 4. Still
                    currentTime < exitStartTime -> {
                        alpha = 1f
                        revealProgress = 1f
                        scale = 1.0f
                    }

                    // 5. Exit phase (1.0 -> 0.0)
                    else -> {
                        val progress = ((endTimeMs - currentTime) / exitDuration).coerceIn(0f, 1f)
                        val eased = FastOutSlowInEasing.transform(progress)
                        alpha = eased
                        scale = eased
                        revealProgress = 1f
                    }
                }
            }

            drawIntoCanvas { canvas ->
                val paint = Paint()
                canvas.saveLayer(Rect(Offset.Zero, Size(totalWidth, size)), paint)

                withTransform({
                    this.scale(
                        scale = scale,
                        pivot = Offset(totalWidth / 2f, size / 2f)
                    )
                }) {
                    repeat(defaults.number) { index ->
                        val dotAlpha: Float
                        if (breathingDuration > 0 && currentTime >= breathingStartTime) {
                            val dotDurationMs = breathingDuration / defaults.number
                            val dotStartTimeInPhase = breathingStartTime + index * dotDurationMs
                            val progressForAlpha = ((currentTime - dotStartTimeInPhase) / dotDurationMs).coerceIn(0f, 1f)
                            dotAlpha = 0.4f + 0.6f * LinearEasing.transform(progressForAlpha)
                        } else {
                            dotAlpha = 0.4f
                        }

                        drawCircle(
                            color = defaults.breathingDotsColor.copy(alpha = dotAlpha * alpha),
                            radius = size / 2,
                            center = Offset(size / 2 + (size + margin) * index, size / 2)
                        )
                    }
                }

                val softEdgeWidth = 0.5f
                val revealPosition = revealProgress * (1f + softEdgeWidth)
                val startFade = (revealPosition - softEdgeWidth).coerceIn(0f, 1f)
                val endFade = revealPosition.coerceIn(0f, 1f)

                if (defaults.breathingDotsColor != Color.Black) {
                    val brush = Brush.horizontalGradient(
                        colorStops = arrayOf(
                            0f to Color.Black,
                            startFade to Color.Black,
                            endFade to Color.Transparent,
                            1f to Color.Transparent
                        )
                    )

                    drawRect(brush = brush, blendMode = BlendMode.DstIn)
                }
                canvas.restore()
            }
        }
    }
}
