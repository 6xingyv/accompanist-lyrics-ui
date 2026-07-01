package com.mocharealm.accompanist.sample.ui.utils.modifier

import androidx.compose.ui.Modifier

actual fun Modifier.deviceRotation(
    rotationFactor: Float,
    cameraDistance: Float
): Modifier = this