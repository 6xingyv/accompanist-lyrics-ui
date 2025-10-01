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
import androidx.compose.ui.graphics.DefaultAlpha
import androidx.compose.ui.graphics.FilterQuality
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.drawscope.DrawScope.Companion.DefaultFilterQuality
import androidx.compose.ui.graphics.painter.BitmapPainter
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import kotlin.math.ceil

@Suppress("DEPRECATION")
fun blurBitmapWithRenderScript(context: Context, bitmap: Bitmap, radius: Float): Bitmap {
    val rs = RenderScript.create(context)
    val input = Allocation.createFromBitmap(rs, bitmap)
    val output = Allocation.createTyped(rs, input.type)
    val script = ScriptIntrinsicBlur.create(rs, Element.U8_4(rs))
    script.setRadius(radius.coerceIn(0f, 25f))
    script.setInput(input)
    script.forEach(output)
    val result = createBitmap(bitmap.width, bitmap.height, bitmap.config ?: Bitmap.Config.ARGB_8888)
    output.copyTo(result)
    rs.destroy()
    return result
}

fun blurBitmapUnbounded(context: Context, bitmap: Bitmap, radius: Float): Bitmap {
    val padding = ceil(radius.toDouble()).toInt()
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

    return blurBitmapWithRenderScript(context, paddedBitmap, radius)
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
            modifier = modifier.blur(blurRadius, BlurredEdgeTreatment.Unbounded),
            alignment = alignment,
            contentScale = contentScale,
            alpha = alpha,
            colorFilter = colorFilter,
            filterQuality = filterQuality
        )
    }
    else {
        val context = LocalContext.current
        val blurRadiusPx = blurRadius.value
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