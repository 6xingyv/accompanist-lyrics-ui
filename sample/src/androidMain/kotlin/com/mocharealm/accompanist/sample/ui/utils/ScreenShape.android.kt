package com.mocharealm.accompanist.sample.ui.utils

import android.os.Build
import android.view.RoundedCorner
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.unit.dp

@Composable
actual fun rememberScreenCornerDataDp(): ScreenCornerDataDp {
    val view = LocalView.current
    val density = LocalDensity.current

    return remember(view, view.rootWindowInsets, density) {
        val insets =
            view.rootWindowInsets ?: return@remember ScreenCornerDataDp(0.dp, 0.dp, 0.dp, 0.dp)

        if (Build.VERSION.SDK_INT > Build.VERSION_CODES.S) {
            val topLeftRadiusPx =
                insets.getRoundedCorner(RoundedCorner.POSITION_TOP_LEFT)?.radius ?: 0
            val topRightRadiusPx =
                insets.getRoundedCorner(RoundedCorner.POSITION_TOP_RIGHT)?.radius ?: 0
            val bottomLeftRadiusPx =
                insets.getRoundedCorner(RoundedCorner.POSITION_BOTTOM_LEFT)?.radius ?: 0
            val bottomRightRadiusPx =
                insets.getRoundedCorner(RoundedCorner.POSITION_BOTTOM_RIGHT)?.radius ?: 0

            val topLeftDp = with(density) { topLeftRadiusPx.toDp() }
            val topRightDp = with(density) { topRightRadiusPx.toDp() }
            val bottomLeftDp = with(density) { bottomLeftRadiusPx.toDp() }
            val bottomRightDp = with(density) { bottomRightRadiusPx.toDp() }

            // Create ScreenCornerDataDp
            ScreenCornerDataDp(
                topLeft = topLeftDp,
                topRight = topRightDp,
                bottomLeft = bottomLeftDp,
                bottomRight = bottomRightDp
            )
        } else {
            ScreenCornerDataDp(
                topLeft = 0.dp,
                topRight = 0.dp,
                bottomLeft = 0.dp,
                bottomRight = 0.dp
            )
        }
    }
}