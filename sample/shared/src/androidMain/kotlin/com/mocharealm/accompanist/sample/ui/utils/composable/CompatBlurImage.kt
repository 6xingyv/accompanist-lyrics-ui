@file:Suppress("DEPRECATION")

package com.mocharealm.accompanist.sample.ui.utils.composable

import android.content.Context
import android.graphics.Bitmap
import android.graphics.Bitmap.createBitmap
import android.graphics.Canvas
import android.os.Build
import android.renderscript.Allocation
import android.renderscript.Element
import android.renderscript.RenderScript
import android.renderscript.ScriptIntrinsicBlur
import androidx.compose.foundation.Image
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.BlurredEdgeTreatment
import androidx.compose.ui.draw.blur
import androidx.compose.ui.graphics.ColorFilter
import androidx.compose.ui.graphics.FilterQuality
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.painter.BitmapPainter
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.util.fastRoundToInt
import kotlin.math.ceil
import kotlin.math.sqrt

@Suppress("DEPRECATION")
fun blurBitmapWithRenderScript(context: Context, bitmap: Bitmap, radius: Float, rsProvided: RenderScript? = null): Bitmap {
    val rs = rsProvided ?: RenderScript.create(context)
    try {
        val input = Allocation.createFromBitmap(rs, bitmap)
        val output = Allocation.createTyped(rs, input.type)
        val script = ScriptIntrinsicBlur.create(rs, Element.U8_4(rs))
        script.setRadius(radius.coerceIn(0f, 25f))
        script.setInput(input)
        script.forEach(output)
        val result = createBitmap(bitmap.width, bitmap.height, bitmap.config ?: Bitmap.Config.ARGB_8888)
        output.copyTo(result)
        return result
    } finally {
        if (rsProvided == null) {
            rs.destroy()
        }
    }
}

/**
 * Support radii greater than 25 by performing multiple blur passes. We choose the number of passes
 * n = ceil((radius / 25)^2) so that each pass radius = radius / sqrt(n) <= 25.
 * Cap the number of passes to avoid excessive work for extreme radii.
 */
@Suppress("DEPRECATION")
fun blurBitmapWithRenderScriptMultiPass(context: Context, bitmap: Bitmap, radius: Float): Bitmap {
    if (radius <= 0f) return bitmap
    if (radius <= 25f) return blurBitmapWithRenderScript(context, bitmap, radius)

    // Number of passes needed so per-pass radius <= 25
    val passes = ceil((radius / 25f) * (radius / 25f)).toInt()
    val passRadius = radius / sqrt(passes.toFloat())

    var current = bitmap
    val rs = RenderScript.create(context)
    try {
        repeat(passes) {
            current = blurBitmapWithRenderScript(context, current, passRadius, rs)
        }
    } finally {
        rs.destroy()
    }
    return current
}

fun blurBitmapUnbounded(context: Context, bitmap: Bitmap, radius: Float): Bitmap {
    val padding = ceil(radius.toDouble()).fastRoundToInt()
    val newWidth = bitmap.width + padding * 2
    val newHeight = bitmap.height + padding * 2

    val paddedBitmap = createBitmap(newWidth, newHeight, bitmap.config ?: Bitmap.Config.ARGB_8888)

    val canvas = Canvas(paddedBitmap)
    canvas.drawBitmap(
        bitmap,
        padding.toFloat(), // left
        padding.toFloat(), // top
        null // paint
    )

    return blurBitmapWithRenderScriptMultiPass(context, paddedBitmap, radius)
}


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
) {
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S && blurRadius > 0.dp) {
        Image(
            bitmap = bitmap,
            contentDescription = contentDescription,
            modifier = Modifier.blur(blurRadius, BlurredEdgeTreatment.Unbounded).then(modifier),
            alignment = alignment,
            contentScale = contentScale,
            alpha = alpha,
            colorFilter = colorFilter,
            filterQuality = filterQuality
        )
    }
    else {
        val context = LocalContext.current
        val blurRadiusPx = with(LocalDensity.current) {blurRadius.toPx()}
        val blurredBitmap = remember(bitmap) {
            blurBitmapUnbounded(
                context,
                bitmap.asAndroidBitmap(),
                blurRadiusPx
            ).asImageBitmap()
        }
        val bitmapPainter = remember(blurredBitmap) { BitmapPainter(blurredBitmap, filterQuality = filterQuality) }
        Image(
            painter = bitmapPainter,
            contentDescription = contentDescription,
            modifier = modifier,
            alignment = alignment,
            contentScale = contentScale,
            alpha = alpha,
            colorFilter = colorFilter,
        )
    }
}