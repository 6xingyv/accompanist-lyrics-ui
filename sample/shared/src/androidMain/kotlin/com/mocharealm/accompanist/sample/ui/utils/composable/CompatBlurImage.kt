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
import androidx.compose.runtime.getValue
import androidx.compose.runtime.produceState
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
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
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

/** Working copies never need to exceed this on their longest edge — at the large
 *  radii the flowing background asks for, blurring a small copy at a proportionally
 *  reduced radius and upscaling is visually identical to blurring at full size. */
private const val MAX_BLUR_INPUT_DIMENSION = 128

/**
 * Large radii made [blurBitmapWithRenderScriptMultiPass] explode: passes grow with
 * (radius / 25)^2, so a 150.dp radius meant hundreds of sequential RenderScript
 * passes. Instead, downscale so the scaled radius fits a single <= 25px pass
 * (also capping the input size), blur once, and upscale back to the exact padded
 * size the full-resolution path would have produced.
 */
fun blurBitmapUnboundedDownscaled(context: Context, bitmap: Bitmap, radius: Float): Bitmap {
    if (radius <= 0f) return bitmap

    val maxDimension = maxOf(bitmap.width, bitmap.height)
    val scale = minOf(
        1f,
        MAX_BLUR_INPUT_DIMENSION.toFloat() / maxDimension,
        25f / radius
    )
    if (scale >= 1f) return blurBitmapUnbounded(context, bitmap, radius)

    val scaledInput = Bitmap.createScaledBitmap(
        bitmap,
        (bitmap.width * scale).fastRoundToInt().coerceAtLeast(1),
        (bitmap.height * scale).fastRoundToInt().coerceAtLeast(1),
        true
    )
    val blurred = blurBitmapUnbounded(context, scaledInput, radius * scale)

    // Match the geometry of the un-scaled path so painters/layout see the same
    // intrinsic size: original bounds plus the full-resolution blur padding.
    val padding = ceil(radius.toDouble()).fastRoundToInt()
    return Bitmap.createScaledBitmap(
        blurred,
        bitmap.width + padding * 2,
        bitmap.height + padding * 2,
        true
    )
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
        // Blur off the main thread (the synchronous remember{} version froze the UI
        // for the whole multipass run). Until the blurred copy is ready — or while a
        // bitmap/radius change is being recomputed — the previous value is shown
        // (initially the sharp bitmap). Keyed on the radius too: remember(bitmap)
        // alone missed radius changes.
        val blurredBitmap by produceState(initialValue = bitmap, bitmap, blurRadiusPx) {
            value = withContext(Dispatchers.Default) {
                blurBitmapUnboundedDownscaled(
                    context,
                    bitmap.asAndroidBitmap(),
                    blurRadiusPx
                ).asImageBitmap()
            }
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