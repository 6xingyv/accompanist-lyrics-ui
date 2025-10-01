package com.mocharealm.accompanist.sample.ui.utils.modifier

import androidx.compose.ui.Modifier

expect fun Modifier.deviceRotation(
    rotationFactor: Float = 0.2f,
    cameraDistance: Float = 12f
): Modifier