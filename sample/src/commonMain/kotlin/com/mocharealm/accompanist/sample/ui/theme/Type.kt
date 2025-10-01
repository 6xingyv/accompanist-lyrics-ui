package com.mocharealm.accompanist.sample.ui.theme

import androidx.compose.material3.Typography
import androidx.compose.runtime.Composable
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.sp
import com.mocharealm.accompanist.lyrics.ui.theme.SFPro

@Composable
fun Typography() : Typography= Typography(
    displayLarge = TextStyle(fontFamily = SFPro()),
    displayMedium = TextStyle(fontFamily = SFPro()),
    displaySmall = TextStyle(fontFamily = SFPro()),
    headlineLarge = TextStyle(fontFamily = SFPro()),
    headlineMedium = TextStyle(fontFamily = SFPro()),
    headlineSmall = TextStyle(fontFamily = SFPro()),
    titleLarge = TextStyle(fontFamily = SFPro()),
    titleMedium = TextStyle(fontFamily = SFPro()),
    titleSmall = TextStyle(fontFamily = SFPro()),
    bodyLarge = TextStyle(
        fontFamily = SFPro(),
        fontWeight = FontWeight.Normal,
        fontSize = 16.sp,
        lineHeight = 24.sp,
        letterSpacing = 0.5.sp
    ),
    bodyMedium = TextStyle(fontFamily = SFPro()),
    bodySmall = TextStyle(fontFamily = SFPro()),
    labelLarge = TextStyle(fontFamily = SFPro()),
    labelMedium = TextStyle(fontFamily = SFPro()),
    labelSmall = TextStyle(fontFamily = SFPro())
)



