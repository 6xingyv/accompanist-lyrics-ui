package com.mocharealm.accompanist.sample.ui.utils.modifier

import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.AnimationSpec
import androidx.compose.animation.core.tween
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.GraphicsLayerScope
import androidx.compose.ui.hapticfeedback.HapticFeedback
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.input.pointer.PointerEvent
import androidx.compose.ui.input.pointer.PointerEventPass
import androidx.compose.ui.input.pointer.PointerEventType
import androidx.compose.ui.layout.Measurable
import androidx.compose.ui.layout.MeasureResult
import androidx.compose.ui.layout.MeasureScope
import androidx.compose.ui.node.CompositionLocalConsumerModifierNode
import androidx.compose.ui.node.DelegatingNode
import androidx.compose.ui.node.LayoutModifierNode
import androidx.compose.ui.node.ModifierNodeElement
import androidx.compose.ui.node.PointerInputModifierNode
import androidx.compose.ui.node.currentValueOf
import androidx.compose.ui.platform.InspectorInfo
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.IntSize
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlin.collections.firstOrNull
import kotlin.let
import kotlin.ranges.coerceIn

class ClickFeedbackNode(
    var pressScale: Float,
    var hoverScale: Float,
    var pressAnimationSpec: AnimationSpec<Float>,
    var releaseAnimationSpec: AnimationSpec<Float>,
    var hoverAnimationSpec: AnimationSpec<Float>,
    var maxHoverOffset: Float,
    var onPress: () -> Unit,
    var onClick: () -> Unit
) : DelegatingNode(), CompositionLocalConsumerModifierNode, LayoutModifierNode,
    PointerInputModifierNode {

    private val animatedScale = Animatable(1f)
    private val animatedOffsetX = Animatable(0f)
    private val animatedOffsetY = Animatable(0f)
    private var isPressed = false
    private var pressJob: Job? = null
    private var hasTriggeredLongPress = false
    private var isHovered = false
    private var haptics: HapticFeedback? = null

    private val layerBlock: GraphicsLayerScope.() -> Unit = {
        scaleX = animatedScale.value
        scaleY = animatedScale.value
        translationX = animatedOffsetX.value
        translationY = animatedOffsetY.value
    }

    override fun onPointerEvent(
        pointerEvent: PointerEvent,
        pass: PointerEventPass,
        bounds: IntSize
    ) {
        if (pass == PointerEventPass.Main) {
            when (pointerEvent.type) {
                PointerEventType.Press -> {
                    isPressed = true
                    hasTriggeredLongPress = false
                    coroutineScope.launch {
                        animatedScale.animateTo(pressScale, pressAnimationSpec)
                    }
                    pressJob = coroutineScope.launch {
                        delay(500)
                        if (isPressed) {
                            hasTriggeredLongPress = true
                            haptics?.performHapticFeedback(HapticFeedbackType.LongPress)
                            onPress()
                        }
                    }
                }

                PointerEventType.Release -> {
                    if (isPressed) {
                        isPressed = false
                        pressJob?.cancel()
                        pressJob = null

                        coroutineScope.launch {
                            val targetScale = if (isHovered) hoverScale else 1f
                            animatedScale.animateTo(targetScale, releaseAnimationSpec)
                        }
                        if (!hasTriggeredLongPress) {
                            onClick()
                        }
                    }
                }

                PointerEventType.Enter -> {
                    isHovered = true
                    if (!isPressed) {
                        coroutineScope.launch {
                            animatedScale.animateTo(hoverScale, hoverAnimationSpec)
                        }
                    }
                }

                PointerEventType.Exit -> {
                    isHovered = false
                    if (!isPressed) {
                        coroutineScope.launch {
                            launch { animatedScale.animateTo(1f, hoverAnimationSpec) }
                            launch { animatedOffsetX.animateTo(0f, hoverAnimationSpec) }
                            launch { animatedOffsetY.animateTo(0f, hoverAnimationSpec) }
                        }
                    }
                }

                PointerEventType.Move -> {
                    if (isHovered && !isPressed) {
                        val position = pointerEvent.changes.firstOrNull()?.position
                        position?.let { pos ->
                            val centerX = bounds.width / 2f
                            val centerY = bounds.height / 2f

                            // Calculate offset relative to center
                            val offsetX = (pos.x - centerX) / centerX
                            val offsetY = (pos.y - centerY) / centerY

                            // Limit offset range and apply maximum offset
                            val clampedOffsetX = offsetX.coerceIn(-1f, 1f) * maxHoverOffset
                            val clampedOffsetY = offsetY.coerceIn(-1f, 1f) * maxHoverOffset

                            // Set values directly without animation for immediate following effect
                            coroutineScope.launch {
                                animatedOffsetX.snapTo(clampedOffsetX)
                                animatedOffsetY.snapTo(clampedOffsetY)
                            }
                        }
                    }
                }
            }
        }
    }

    override fun onCancelPointerInput() {
        isPressed = false
        coroutineScope.launch {
            val targetScale = if (isHovered) hoverScale else 1f
            launch { animatedScale.animateTo(targetScale, releaseAnimationSpec) }
            launch { animatedOffsetX.animateTo(0f, releaseAnimationSpec) }
            launch { animatedOffsetY.animateTo(0f, releaseAnimationSpec) }
        }
    }

    override fun MeasureScope.measure(
        measurable: Measurable,
        constraints: Constraints
    ): MeasureResult {
        val placeable = measurable.measure(constraints)
        haptics = currentValueOf(LocalHapticFeedback)
        return layout(placeable.width, placeable.height) {
            placeable.placeWithLayer(0, 0, layerBlock = layerBlock)
        }
    }
}

private data class ClickFeedbackElement(
    val pressScale: Float,
    val hoverScale: Float,
    val pressAnimationSpec: AnimationSpec<Float>,
    val releaseAnimationSpec: AnimationSpec<Float>,
    val hoverAnimationSpec: AnimationSpec<Float>,
    val maxHoverOffset: Float = 8f,
    val onPress: () -> Unit,
    val onClick: () -> Unit
) : ModifierNodeElement<ClickFeedbackNode>() {
    override fun InspectorInfo.inspectableProperties() {
        name = "ClickFeedback"
        properties["pressScale"] = pressScale
        properties["hoverScale"] = hoverScale
        properties["pressAnimationSpec"] = pressAnimationSpec
        properties["releaseAnimationSpec"] = releaseAnimationSpec
        properties["hoverAnimationSpec"] = hoverAnimationSpec
        properties["maxHoverOffset"] = maxHoverOffset
        properties["onPress"] = onPress
        properties["onClick"] = onClick
    }


    override fun create(): ClickFeedbackNode {
        return ClickFeedbackNode(
            pressScale, hoverScale, pressAnimationSpec, releaseAnimationSpec,
            hoverAnimationSpec, maxHoverOffset,
            onPress, onClick
        )
    }

    override fun update(node: ClickFeedbackNode) {
        node.pressScale = pressScale
        node.hoverScale = hoverScale
        node.pressAnimationSpec = pressAnimationSpec
        node.releaseAnimationSpec = releaseAnimationSpec
        node.hoverAnimationSpec = hoverAnimationSpec
        node.maxHoverOffset = maxHoverOffset
        node.onPress = onPress
        node.onClick = onClick
    }
}

fun Modifier.clickFeedback(
    pressScale: Float = 0.95f,
    hoverScale: Float = 1.05f,
    pressAnimationSpec: AnimationSpec<Float> = tween(100),
    releaseAnimationSpec: AnimationSpec<Float> = tween(400),
    hoverAnimationSpec: AnimationSpec<Float> = tween(400),
    maxHoverOffset: Float = 8f,
    onPress: () -> Unit = {},
    onClick: () -> Unit
): Modifier = this then ClickFeedbackElement(
    pressScale, hoverScale, pressAnimationSpec, releaseAnimationSpec,
    hoverAnimationSpec, maxHoverOffset,
    onPress, onClick
)