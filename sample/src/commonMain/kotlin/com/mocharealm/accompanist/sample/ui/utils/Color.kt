package com.mocharealm.accompanist.sample.ui.utils

import androidx.compose.ui.graphics.Color
import kotlin.math.max
import kotlin.math.min

fun Color.copyHsl(
    hue: Float? = null,
    saturation: Float? = null,
    lightness: Float? = null,
    alpha: Float? = null
): Color {
    // 原始 RGBA 转 [0,1]
    val r = red
    val g = green
    val b = blue

    // 先把 RGB 转 HSL
    val maxVal = max(r, max(g, b))
    val minVal = min(r, min(g, b))
    val l = (maxVal + minVal) / 2f

    val s: Float
    val h: Float

    if (maxVal == minVal) {
        h = 0f
        s = 0f
    } else {
        val d = maxVal - minVal
        s = if (l > 0.5f) d / (2f - maxVal - minVal) else d / (maxVal + minVal)
        h = when (maxVal) {
            r -> (g - b) / d + (if (g < b) 6 else 0)
            g -> (b - r) / d + 2
            else -> (r - g) / d + 4
        } / 6f
    }

    // 使用传入值（注意 hue 是 0–360，s/l 是 0–1）
    val newHue = hue ?: (h * 360f)
    val newSaturation = saturation ?: s
    val newLightness = lightness ?: l
    val newAlpha = alpha ?: this.alpha

    // 再把 HSL 转回 RGB
    val c = (1f - kotlin.math.abs(2f * newLightness - 1f)) * newSaturation
    val x = c * (1f - kotlin.math.abs((newHue / 60f) % 2f - 1f))
    val m = newLightness - c / 2f

    val (rr, gg, bb) = when {
        newHue < 60f -> Triple(c, x, 0f)
        newHue < 120f -> Triple(x, c, 0f)
        newHue < 180f -> Triple(0f, c, x)
        newHue < 240f -> Triple(0f, x, c)
        newHue < 300f -> Triple(x, 0f, c)
        else -> Triple(c, 0f, x)
    }

    return Color(
        red = rr + m,
        green = gg + m,
        blue = bb + m,
        alpha = newAlpha
    )
}
