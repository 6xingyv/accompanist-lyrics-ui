package com.mocharealm.accompanist.lyrics.ui.internal.effects

import androidx.compose.animation.core.MutableTransitionState
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.rememberTransition
import androidx.compose.foundation.layout.Box
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.TransformOrigin
import androidx.compose.ui.layout.layout
import androidx.compose.ui.unit.constrainHeight
import com.mocharealm.accompanist.lyrics.ui.preparation.LineVariantBinding
import com.mocharealm.accompanist.lyrics.ui.preparation.PreparedLine
import kotlin.math.roundToInt

/**
 * One spring per line describes how much of the pronunciation-aligned geometry is still applied:
 * 1 is fully aligned, 0 is fully compact. Units keep their row slide towards their target x with
 * it; units whose row changes fade instead.
 */
internal class LineVariantTransition(
    val aligned: PreparedLine,
    val compact: PreparedLine,
    private val visibility: MutableTransitionState<Boolean>,
    val progress: State<Float>,
) {
    val binding: LineVariantBinding? = aligned.binding

    /** Geometry still occupying the line; flips once the spring has settled. */
    val currentAligned: Boolean
        get() = visibility.currentState

    /** Geometry the spring is heading to. */
    val targetAligned: Boolean
        get() = visibility.targetState
}

@Composable
internal fun rememberLineVariantTransition(
    line: PreparedLine,
    alignedPhonetics: Boolean,
): LineVariantTransition? {
    val compact = line.compact ?: return null
    val binding = line.binding ?: return null
    val visibility = remember(line) { MutableTransitionState(alignedPhonetics) }
    visibility.targetState = alignedPhonetics
    val captionTransitions = LocalLyricsCaptionTransitions.current
    if (captionTransitions != null) {
        // Follow-scroll waits for captions to settle; a variant switch reflows the same rows.
        DisposableEffect(captionTransitions, visibility) {
            captionTransitions.states[visibility] = visibility
            onDispose { captionTransitions.states.remove(visibility) }
        }
    }
    val transition = rememberTransition(visibility, label = "lyricsLineVariant")
    val progress =
        transition.animateFloat(
            transitionSpec = { LyricsRevealSpring },
            label = "lyricsLineVariantProgress",
        ) {
            if (it) 1f else 0f
        }
    return remember(line, compact, binding, visibility) {
        LineVariantTransition(line, compact, visibility, progress)
    }
}

/**
 * The caption reveal, driven by an existing spring. Rows that only exist in one variant own their
 * height the same way captions do, so the rest of the line reflows with them instead of jumping.
 */
@Composable
internal fun LyricsRevealProgress(
    progress: () -> Float,
    origin: TransformOrigin = TransformOrigin.Center,
    content: @Composable () -> Unit,
) {
    Box(
        Modifier.layout { measurable, constraints ->
                val placeable = measurable.measure(constraints.copy(minHeight = 0))
                val reveal = progress().coerceIn(0f, 1f)
                val height = constraints.constrainHeight((placeable.height * reveal).roundToInt())
                val offset = ((height - placeable.height) * origin.pivotFractionY).roundToInt()
                layout(placeable.width, height) { placeable.place(0, offset) }
            }
            .lyricsVisualLayer(observePaint = { progress() }) {
                val reveal = progress().coerceIn(0f, 1f)
                transformOrigin = origin
                scaleX = revealScale(reveal)
                scaleY = scaleX
                alpha = revealAlpha(reveal)
            }
    ) {
        content()
    }
}
