package com.mocharealm.accompanist.sample.ui.composable

import androidx.compose.ui.unit.IntSize
import kotlin.math.min
import kotlin.math.roundToInt

/** Physical-pixel size for ContentScale.Fit; never enlarge the encoded source. */
fun artworkSize(source: IntSize, viewport: IntSize): IntSize {
    require(source.width > 0 && source.height > 0 && viewport.width > 0 && viewport.height > 0)
    val scale =
        min(
            1f,
            min(viewport.width.toFloat() / source.width, viewport.height.toFloat() / source.height),
        )
    return IntSize(
        (source.width * scale).roundToInt().coerceAtLeast(1),
        (source.height * scale).roundToInt().coerceAtLeast(1),
    )
}
