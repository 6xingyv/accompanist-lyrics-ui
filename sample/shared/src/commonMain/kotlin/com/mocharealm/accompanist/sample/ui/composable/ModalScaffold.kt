package com.mocharealm.accompanist.sample.ui.composable

import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.AnimationSpec
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.detectVerticalDragGestures
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.asPaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.sizeIn
import androidx.compose.foundation.layout.statusBars
import androidx.compose.foundation.layout.systemBarsPadding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.RectangleShape
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalWindowInfo
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import com.mocharealm.accompanist.lyrics.ui.utils.copyHsl
import com.mocharealm.accompanist.sample.ui.adaptive.LocalWindowLayoutType
import com.mocharealm.accompanist.sample.ui.adaptive.WindowLayoutType
import com.mocharealm.accompanist.sample.ui.utils.ScreenCornerDataDp
import com.mocharealm.accompanist.sample.ui.utils.composable.CompatBackHandler
import com.mocharealm.accompanist.sample.ui.utils.rememberScreenCornerDataDp
import com.mocharealm.gaze.capsule.ContinuousRoundedRectangle
import kotlinx.coroutines.launch
import kotlin.coroutines.cancellation.CancellationException
import kotlin.math.roundToInt

@Composable
fun ModalScaffold(
    isModalOpen: Boolean,
    onDismissRequest: () -> Unit,
    modifier: Modifier = Modifier,
    confirmDismiss: () -> Boolean = { true },
    screenCornerDataDp: ScreenCornerDataDp = rememberScreenCornerDataDp(),
    targetRadius: Dp = 16.dp,
    animationSpec: AnimationSpec<Float> = tween(durationMillis = 400),
    dismissThresholdFraction: Float = 0.5f,
    modalContent: @Composable (dragHandleModifier: Modifier) -> Unit,
    content: @Composable () -> Unit
) {
    BoxWithConstraints {
        // A landscape phone is still classified as `Phone` (shortest-edge breakpoints),
        // but the bottom sheet makes no sense on such a short screen — use the
        // centered pad dialog whenever the window is wider than it is tall.
        val isLandscape = maxWidth > maxHeight
        val layoutType = LocalWindowLayoutType.current
        if (layoutType == WindowLayoutType.Phone && !isLandscape) {
            MobileModalScaffold(
                isModalOpen = isModalOpen,
                onDismissRequest = onDismissRequest,
                modifier = modifier,
                confirmDismiss = confirmDismiss,
                screenCornerDataDp = screenCornerDataDp,
                targetRadius = targetRadius,
                animationSpec = animationSpec,
                dismissThresholdFraction = dismissThresholdFraction,
                modalContent = modalContent,
                content = content
            )
        } else {
            // Tablet, Desktop, Tv and landscape phones all use the centered dialog.
            PadModalScaffold(
                isModalOpen = isModalOpen,
                onDismissRequest = onDismissRequest,
                modifier = modifier,
                confirmDismiss = confirmDismiss,
                targetRadius = targetRadius,
                animationSpec = animationSpec,
                modalContent = modalContent,
                content = content
            )
        }
    }
}

@Composable
fun MobileModalScaffold(
    isModalOpen: Boolean,
    onDismissRequest: () -> Unit,
    modifier: Modifier = Modifier,
    confirmDismiss: () -> Boolean = { true },
    screenCornerDataDp: ScreenCornerDataDp = rememberScreenCornerDataDp(),
    targetRadius: Dp = 16.dp,
    animationSpec: AnimationSpec<Float> = tween(durationMillis = 400),
    dismissThresholdFraction: Float = 0.5f,
    modalContent: @Composable (dragHandleModifier: Modifier) -> Unit,
    content: @Composable () -> Unit
) {
    val scope = rememberCoroutineScope()
    val windowInfo = LocalWindowInfo.current
    val density = LocalDensity.current

    val screenHeightDp = with(density) { windowInfo.containerSize.height.toDp() }

    val height = with(density) {
        LocalWindowInfo.current.containerSize.height.toDp()
    }

    val backgroundScale =
        ((height / 2 - WindowInsets.statusBars.asPaddingValues()
            .calculateTopPadding()) / (height / 2)).coerceAtMost(
            0.95f
        )

    // 1. 使用 Animatable 来管理垂直偏移量
    val offsetY = remember { Animatable(0f) }
    var modalHeight by remember { mutableFloatStateOf(0f) }
    // offsetY starts at 0 and is only snapped to modalHeight once the sheet has been
    // measured — until that snap has happened the sheet must not render (and the page
    // must stay un-scaled/un-dimmed) or the first frame flashes with the sheet fully up.
    var heightMeasured by remember { mutableStateOf(false) }

    // 2. 监听 isModalOpen 的变化，驱动模态窗口的开合动画
    LaunchedEffect(isModalOpen, modalHeight) {
        if (modalHeight == 0f) return@LaunchedEffect
        val targetValue = if (isModalOpen) 0f else modalHeight
        if (offsetY.value != targetValue) {
            scope.launch {
                offsetY.animateTo(targetValue, animationSpec)
            }
        }
    }

    CompatBackHandler(enabled = isModalOpen) { progressFlow ->
        try {
            progressFlow.collect { backEvent ->
                offsetY.snapTo(backEvent.progress * modalHeight)
            }
            // Animate the sheet fully out BEFORE notifying dismissal (same ordering as
            // drag-dismiss) — otherwise the callback resets the sheet's content while
            // it is still partially on screen and an empty sheet visibly slides out.
            offsetY.animateTo(modalHeight, animationSpec)
            onDismissRequest()
        } catch (_: CancellationException) {
            offsetY.animateTo(0f, animationSpec)
        }
    }

    // 3. 计算动画进度。These helpers read offsetY.value LAZILY so the per-frame
    // animation value is only observed inside graphicsLayer/draw lambdas — the
    // scaffold (and its content) never recomposes while the sheet animates.
    fun progressValue(): Float = if (heightMeasured && modalHeight > 0) {
        (offsetY.value / modalHeight).coerceIn(0f, 1f)
    } else {
        if (isModalOpen) 0f else 1f
    }

    // Robust closed test: interrupted animations can park offsetY a fraction of a pixel
    // short of modalHeight, so a `progress != 1f` float-equality gate would leave the
    // sheet edge peeking and the page permanently corner-clipped. Treat anything within
    // half a pixel of fully-off-screen as closed.
    fun isFullyClosedNow(): Boolean = if (heightMeasured && modalHeight > 0) {
        modalHeight - offsetY.value < 0.5f
    } else {
        !isModalOpen
    }

    val modalTopPadding = (screenHeightDp * (1 - backgroundScale) / 2f + 16.dp).coerceAtLeast(0.dp)

    // 4. 拖拽手势
    val dragHandleModifier = Modifier.pointerInput(Unit) {
        detectVerticalDragGestures(
            onDragEnd = {
                scope.launch {
                    if (offsetY.value > modalHeight * dismissThresholdFraction) {
                        if (confirmDismiss()) {
                            offsetY.animateTo(modalHeight, animationSpec)
                            onDismissRequest()
                        } else {
                            offsetY.animateTo(0f, animationSpec)
                        }
                    } else {
                        offsetY.animateTo(0f, animationSpec)
                    }
                }
            },
            onVerticalDrag = { change, dragAmount ->
                change.consume()
                scope.launch {
                    val newOffset = (offsetY.value + dragAmount).coerceAtLeast(0f)
                    offsetY.snapTo(newOffset)
                }
            }
        )
    }

    Box(
        modifier = modifier
            .fillMaxSize()
            .background(Color.Black)
    ) {
        Box(
            modifier = Modifier
                .fillMaxSize()
                .graphicsLayer {
                    // Per-frame reads live in this lambda: only the layer is
                    // re-evaluated per animation frame, not the composition. The
                    // shape rebuild is confined here too — it only happens while
                    // the radii are actually lerping.
                    val progress = progressValue()
                    val scale = lerp(backgroundScale, 1f, progress)
                    scaleX = scale
                    scaleY = scale
                    if (isFullyClosedNow()) {
                        clip = false
                        shape = RectangleShape
                    } else {
                        clip = true
                        shape = ContinuousRoundedRectangle(
                            topStart = lerp(targetRadius, screenCornerDataDp.topLeft, progress),
                            topEnd = lerp(targetRadius, screenCornerDataDp.topRight, progress),
                            bottomStart = lerp(targetRadius, screenCornerDataDp.bottomLeft, progress),
                            bottomEnd = lerp(targetRadius, screenCornerDataDp.bottomRight, progress)
                        )
                    }
                }
        ) {
            content()
        }

        Box(
            modifier = Modifier
                .fillMaxSize()
                .drawBehind {
                    val dimAlpha = lerp(0.4f, 0f, progressValue())
                    if (dimAlpha > 0f) {
                        drawRect(Color.Black.copy(alpha = dimAlpha))
                    }
                }
        )

        Box(
            modifier = Modifier
                .offset { IntOffset(0, offsetY.value.roundToInt()) }
                .padding(top = modalTopPadding)
                .fillMaxSize()
                .graphicsLayer {
                    val progress = progressValue()
                    alpha = if (isFullyClosedNow()) 0f else 1f
                    clip = true
                    shape = ContinuousRoundedRectangle(
                        topStart = lerp(targetRadius, screenCornerDataDp.topLeft, progress),
                        topEnd = lerp(targetRadius, screenCornerDataDp.topRight, progress),
                    )
                }
                .background(
                    if (isSystemInDarkTheme())
                        Color.Black.copyHsl(lightness = 0.15f)
                    else Color.White
                )
                .onSizeChanged { size ->
                    if (modalHeight == 0f && !isModalOpen) {
                        scope.launch {
                            offsetY.snapTo(size.height.toFloat())
                            heightMeasured = true
                        }
                    } else {
                        heightMeasured = true
                    }
                    modalHeight = size.height.toFloat()
                },
        ) {
            modalContent(dragHandleModifier)
        }
    }
}

@Composable
fun PadModalScaffold(
    isModalOpen: Boolean,
    onDismissRequest: () -> Unit,
    modifier: Modifier = Modifier,
    confirmDismiss: () -> Boolean = { true },
    targetRadius: Dp = 16.dp,
    animationSpec: AnimationSpec<Float> = tween(durationMillis = 400),
    modalContent: @Composable (dragHandleModifier: Modifier) -> Unit,
    content: @Composable () -> Unit
) {
    val scope = rememberCoroutineScope()
    val progress = remember { Animatable(if (isModalOpen) 0f else 1f) }

    LaunchedEffect(isModalOpen) {
        progress.animateTo(if (isModalOpen) 0f else 1f, animationSpec)
    }

    CompatBackHandler(enabled = isModalOpen) { progressFlow ->
        if (!confirmDismiss()) return@CompatBackHandler
        try {
            progressFlow.collect { backEvent ->
                progress.snapTo(backEvent.progress)
            }
            onDismissRequest()
        } catch (_: CancellationException) {
            scope.launch {
                progress.animateTo(0f, animationSpec)
            }
        }
    }

    Box(
        modifier = modifier
            .fillMaxSize()
            .background(Color.Black)
    ) {
        Box(
            modifier = Modifier
                .fillMaxSize()
        ) {
            content()
        }

        // Robust closed test (epsilon instead of float equality) — and when closed,
        // do not compose the scrim or the modal at all: an alpha-0 modal would still
        // be hit-testable and block clicks on the content underneath.
        // derivedStateOf: recompose only when the boolean flips, not per animation frame.
        val isFullyClosed by remember { derivedStateOf { progress.value > 0.999f } }
        if (!isFullyClosed) {
            Box(
                modifier = Modifier
                    .fillMaxSize()
                    .drawBehind {
                        // Per-frame scrim alpha stays in the draw phase.
                        drawRect(Color.Black.copy(lerp(0.4f, 0f, progress.value)))
                    }
                    .clickable {
                        if (confirmDismiss()) {
                            scope.launch {
                                progress.animateTo(1f, animationSpec)
                            }
                            onDismissRequest()
                        }
                    }
            )

            Box(
                Modifier
                    .align(Alignment.Center)
                    .graphicsLayer {
                        alpha = 1f - progress.value
                        scaleX = (1f - progress.value) * 0.05f + 1f
                        scaleY = (1f - progress.value) * 0.05f + 1f
                    }
                    .systemBarsPadding()
                    .padding(vertical = 20.dp)
                    .clip(ContinuousRoundedRectangle(targetRadius))
                    .background(
                        if (isSystemInDarkTheme())
                            Color.Black.copyHsl(lightness = 0.15f)
                        else Color.White
                    )
                    .sizeIn(maxWidth = 420.dp)
            ) {
                modalContent(Modifier)
            }
        }
    }
}

// 线性插值函数
private fun lerp(start: Float, stop: Float, fraction: Float): Float {
    return (1 - fraction) * start + fraction * stop
}

private fun lerp(start: Dp, stop: Dp, fraction: Float): Dp {
    return Dp(lerp(start.value, stop.value, fraction))
}
