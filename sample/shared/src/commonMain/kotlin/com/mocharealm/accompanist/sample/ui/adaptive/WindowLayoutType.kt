package com.mocharealm.accompanist.sample.ui.adaptive

import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.ReadOnlyComposable
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp

enum class WindowLayoutType {
    Phone,
    Tablet,
    Desktop,
    Tv;

    companion object {
        val current: WindowLayoutType
            @Composable
            @ReadOnlyComposable
            get() = LocalWindowLayoutType.current
    }
}

private object WindowBreakpoints {
    val Tablet: Dp = 600.dp
    val Desktop: Dp = 840.dp
}

val LocalWindowLayoutType = staticCompositionLocalOf<WindowLayoutType> {
    error("No WindowLayoutType provided. Did you forget to wrap your app in AdaptiveLayoutProvider?")
}

/** Whether the app is running on a television (leanback) device. */
@Composable
expect fun isTelevision(): Boolean

@Composable
fun AdaptiveLayoutProvider(
    content: @Composable () -> Unit
) {
    val isTv = isTelevision()
    BoxWithConstraints {
        // Classify by the SHORTER edge (smallest width), not just the width, so a
        // phone stays `Phone` in landscape too — otherwise a phone turned sideways
        // (wide but short) is misread as a Tablet/Desktop and loses the full-bleed
        // fluid-mesh player. A real tablet's shorter edge is still ≥ the breakpoint.
        val shortestEdge = minOf(maxWidth, maxHeight)
        val layoutType = when {
            isTv -> WindowLayoutType.Tv
            shortestEdge < WindowBreakpoints.Tablet -> WindowLayoutType.Phone
            shortestEdge < WindowBreakpoints.Desktop -> WindowLayoutType.Tablet
            else -> WindowLayoutType.Desktop
        }
        CompositionLocalProvider(LocalWindowLayoutType provides layoutType) {
            content()
        }
    }
}