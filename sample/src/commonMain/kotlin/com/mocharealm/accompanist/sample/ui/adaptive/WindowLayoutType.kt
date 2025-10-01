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

private val LocalWindowLayoutType = staticCompositionLocalOf<WindowLayoutType> {
    error("No WindowLayoutType provided. Did you forget to wrap your app in AdaptiveLayoutProvider?")
}

@Composable
fun AdaptiveLayoutProvider(
    content: @Composable () -> Unit
) {
    BoxWithConstraints {
        val layoutType = when {
            maxWidth < WindowBreakpoints.Tablet -> WindowLayoutType.Phone
            maxWidth < WindowBreakpoints.Desktop -> WindowLayoutType.Tablet
            else -> WindowLayoutType.Desktop
        }
        CompositionLocalProvider(LocalWindowLayoutType provides layoutType) {
            content()
        }
    }
}