package com.mocharealm.accompanist.sample.ui.theme

import androidx.compose.material3.Typography
import androidx.compose.runtime.Composable
import androidx.compose.ui.text.ExperimentalTextApi
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontVariation
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.sp
import com.mocharealm.accompanist.sample.Res
import com.mocharealm.accompanist.sample.sf_pro
import org.jetbrains.compose.resources.Font

@OptIn(ExperimentalTextApi::class)
@Composable
fun SFPro(): FontFamily = FontFamily(
    Font(
        resource = Res.font.sf_pro,
        weight = FontWeight.ExtraLight,
        variationSettings = FontVariation.Settings(
            FontVariation.weight(FontWeight.ExtraLight.weight),
        )
    ),
    Font(
        resource = Res.font.sf_pro,
        weight = FontWeight.Light,
        variationSettings = FontVariation.Settings(
            FontVariation.weight(FontWeight.Light.weight),
        )
    ),
    Font(
        resource = Res.font.sf_pro,
        weight = FontWeight.Medium,
        variationSettings = FontVariation.Settings(
            FontVariation.weight(FontWeight.Medium.weight),
        )
    ),
    Font(
        resource = Res.font.sf_pro,
        weight = FontWeight.SemiBold,
        variationSettings = FontVariation.Settings(
            FontVariation.weight(FontWeight.SemiBold.weight),
        )
    ),
    Font(
        resource = Res.font.sf_pro,
        weight = FontWeight.Bold,
        variationSettings = FontVariation.Settings(
            FontVariation.weight(FontWeight.Bold.weight),
        )
    ),
    Font(
        resource = Res.font.sf_pro,
        weight = FontWeight.ExtraBold,
        variationSettings = FontVariation.Settings(
            FontVariation.weight(FontWeight.ExtraBold.weight),
        )
    ),
)

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



