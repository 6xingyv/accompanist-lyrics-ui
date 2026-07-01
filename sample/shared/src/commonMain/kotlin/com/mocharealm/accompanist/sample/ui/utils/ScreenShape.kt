package com.mocharealm.accompanist.sample.ui.utils

import androidx.compose.runtime.Composable
import androidx.compose.ui.unit.Dp


data class ScreenCornerDataDp(
    val topLeft: Dp,
    val topRight:Dp,
    val bottomLeft: Dp,
    val bottomRight: Dp
)


@Composable
expect fun rememberScreenCornerDataDp(): ScreenCornerDataDp