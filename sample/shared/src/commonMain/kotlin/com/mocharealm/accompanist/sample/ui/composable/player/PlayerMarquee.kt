package com.mocharealm.accompanist.sample.ui.composable.player

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.graphics.BlendMode
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Paint
import androidx.compose.ui.graphics.drawscope.clipRect
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.text
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.drawText
import androidx.compose.ui.text.rememberTextMeasurer
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp

// Ported from feature-text-engine: lyrics-renderer/src/renderer/draw.rs.
// These are renderer pixels, not density-scaled dp values.
internal const val MARQUEE_GAP_PX = 48f
internal const val MARQUEE_SCROLL_PX_PER_SEC = 40f
internal const val MARQUEE_HOLD_MS = 1600f

internal fun marqueeCycleDistance(textWidth: Float, viewportWidth: Float,
    leftFadeWidth: Float, rightFadeWidth: Float): Float =
    textWidth + viewportWidth + leftFadeWidth + rightFadeWidth + MARQUEE_GAP_PX

internal fun marqueeOffset(currentTimeMillis: Int, cycleDistance: Float): Float {
    val scrollMillis = (cycleDistance / MARQUEE_SCROLL_PX_PER_SEC * 1000f).coerceAtLeast(1f)
    val period = scrollMillis + MARQUEE_HOLD_MS
    val remainder = currentTimeMillis.toFloat() % period
    val time = if (remainder < 0f) remainder + period else remainder
    if (time < MARQUEE_HOLD_MS) return 0f
    val progress = ((time - MARQUEE_HOLD_MS) / scrollMillis).coerceIn(0f, 1f)
    return progress * progress * (3f - 2f * progress) * cycleDistance
}

/** The clock is sampled during drawing, matching the native renderer's playback clock. */
@Composable
internal fun PlayerMarqueeText(
    value: String,
    style: TextStyle,
    alpha: Float,
    currentTimeMillis: () -> Int,
    modifier: Modifier = Modifier,
    leftFade: Dp = 8.dp,
    rightFade: Dp = 8.dp,
) {
    val measurer = rememberTextMeasurer()
    val layout = measurer.measure(value, style, softWrap = false, maxLines = 1,
        constraints = Constraints())
    val density = LocalDensity.current
    val height = with(density) { layout.size.height.toDp() }
    val plusPaint = remember { Paint().apply { blendMode = BlendMode.Plus } }
    Canvas(modifier.fillMaxWidth().height(height).semantics {
        text = AnnotatedString(value)
    }) {
        if (alpha <= 0f || size.width <= 0f) return@Canvas
        val textWidth = layout.getLineRight(0) - layout.getLineLeft(0)
        val overflowing = textWidth - size.width > .5f
        val left = if (overflowing) leftFade.toPx().coerceAtLeast(0f) else 0f
        val right = if (overflowing) rightFade.toPx().coerceAtLeast(0f) else 0f
        val top = -layout.size.height * .5f
        val bottom = layout.size.height * 1.5f
        val canvas = drawContext.canvas
        canvas.saveLayer(Rect(-left, top, size.width + right, bottom), plusPaint)
        try {
            if (!overflowing) {
                drawText(layout, color = Color.White, alpha = alpha)
                return@Canvas
            }
            val distance = marqueeCycleDistance(textWidth, size.width, left, right)
            val offset = marqueeOffset(currentTimeMillis(), distance)
            clipRect(-left, top, size.width + right, bottom) {
                drawText(layout, color = Color.White, topLeft = Offset(-offset, 0f), alpha = alpha)
                drawText(layout, color = Color.White, topLeft = Offset(distance - offset, 0f), alpha = alpha)
            }
            if (left > 0f || right > 0f) {
                val total = left + size.width + right
                drawRect(
                    Brush.horizontalGradient(
                        0f to if (left > 0f) Color.Transparent else Color.White,
                        left / total to Color.White,
                        (left + size.width) / total to Color.White,
                        1f to if (right > 0f) Color.Transparent else Color.White,
                        startX = -left, endX = size.width + right,
                    ),
                    topLeft = Offset(-left, top), size = androidx.compose.ui.geometry.Size(total, bottom - top),
                    blendMode = BlendMode.DstIn,
                )
            }
        } finally { canvas.restore() }
    }
}
