package com.mocharealm.accompanist.sample.ui.utils.composable

import androidx.compose.foundation.Image
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.BlurredEdgeTreatment
import androidx.compose.ui.draw.blur
import androidx.compose.ui.graphics.ColorFilter
import androidx.compose.ui.graphics.FilterQuality
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.unit.Dp

@Composable
actual fun CompatBlurImage(
    bitmap: ImageBitmap,
    contentDescription: String?,
    modifier: Modifier,
    alignment: Alignment,
    contentScale: ContentScale,
    blurRadius: Dp,
    alpha: Float,
    colorFilter: ColorFilter?,
    filterQuality: FilterQuality
) =
    Image(
        bitmap = bitmap,
        contentDescription = contentDescription,
        modifier = modifier.blur(blurRadius, BlurredEdgeTreatment.Unbounded),
        alignment = alignment,
        contentScale = contentScale,
        alpha = alpha,
        colorFilter = colorFilter,
        filterQuality = filterQuality
    )