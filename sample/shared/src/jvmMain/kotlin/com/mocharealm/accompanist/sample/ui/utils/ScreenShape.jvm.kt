package com.mocharealm.accompanist.sample.ui.utils

import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalWindowInfo
import androidx.compose.ui.unit.dp
import org.jetbrains.skiko.OS
import org.jetbrains.skiko.hostOs

@Composable
actual fun rememberScreenCornerDataDp(): ScreenCornerDataDp {
    val windowInfo = LocalWindowInfo.current
    val density = LocalDensity.current
    return remember(windowInfo, density, hostOs) {
        when (hostOs) {
            OS.MacOS -> {
                // Desktop typically does not have rounded corners
                ScreenCornerDataDp(
                    topLeft = 8.dp,
                    topRight = 8.dp,
                    bottomLeft = 8.dp,
                    bottomRight = 8.dp
                )
            }
            OS.Windows -> {
                ScreenCornerDataDp(
                    topLeft = 8.dp,
                    topRight = 8.dp,
                    bottomLeft = 8.dp,
                    bottomRight = 8.dp
                )
            }
            else -> {
                // Fallback for other platforms
                ScreenCornerDataDp(
                    topLeft = 0.dp,
                    topRight = 0.dp,
                    bottomLeft = 0.dp,
                    bottomRight = 0.dp
                )
            }
        }
    }
}