package com.mocharealm.accompanist.lyrics.ui.composable.lyrics

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp

data class KaraokeBreathingDotsConfigs(
    val number: Int = 3,
    val size: Dp = 16.dp,
    val margin: Dp = 8.dp,
    val enterDurationMs: Int = 3000,
    val preExitStillDuration: Int = 200,
    val preExitDipAndRiseDuration: Int = 3000,
    val exitDurationMs: Int = 200,
    val breathingDotsColor: Color = Color.White
)
