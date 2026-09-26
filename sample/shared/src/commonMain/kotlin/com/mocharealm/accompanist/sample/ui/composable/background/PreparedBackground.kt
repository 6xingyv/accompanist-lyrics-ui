package com.mocharealm.accompanist.sample.ui.composable.background

import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.*
import androidx.compose.ui.graphics.drawscope.CanvasDrawScope
import androidx.compose.ui.unit.*
import kotlin.math.*
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.ensureActive

/** Prepare four blurred layers once. Only the bounded, opaque composite survives preparation. */
internal suspend fun prepareBackground(
    source: ImageBitmap,
    luminance: Float,
    viewport: IntSize,
    density: Float,
): ImageBitmap {
    require(viewport.width > 0 && viewport.height > 0 && density > 0f)
    val ratio = min(1f, 512f / max(viewport.width, viewport.height))
    val width = (viewport.width * ratio).roundToInt().coerceAtLeast(1)
    val height = (viewport.height * ratio).roundToInt().coerceAtLeast(1)
    val dp = density * ratio
    val baseWidth = min(width.toFloat(), 800f * dp)
    val baseHeight = min(height.toFloat(), 800f * dp)
    val result = IntArray(width * height) { 0xff000000.toInt() }
    val filter = ColorFilter.colorMatrix(
        saturationAndValueMatrix(
            saturation =  1.8f,
            valueScale = 0.5f,
        )
    )
    for (layer in 0..3) {
        currentCoroutineContext().ensureActive()
        val boxWidth = if (layer == 0) width.toFloat() else baseWidth
        val boxHeight = if (layer == 0) height.toFloat() else baseHeight
        val scale =
            when (layer) {
                0,
                1 -> 2.5f
                2 -> 1.5f
                else -> 2f
            }
        val centerX =
            when (layer) {
                1 -> baseWidth / 2 - 150f * dp
                3 -> width - baseWidth / 2 + 150f * dp
                else -> width / 2f
            }
        val centerY =
            when (layer) {
                1 -> baseHeight / 2 - 150f * dp
                3 -> height - baseHeight / 2 + 150f * dp
                else -> height / 2f
            }
        val fit = min(boxWidth / source.width, boxHeight / source.height) * scale
        val imageWidth = (source.width * fit).roundToInt().coerceAtLeast(1)
        val imageHeight = (source.height * fit).roundToInt().coerceAtLeast(1)
        val sigma =
            (when (layer) {
                0 -> 25f
                1 -> 80f
                2 -> 100f
                else -> 150f
            }) * dp
        // Three box passes approximate Gaussian blur. Include off-screen pixels in
        // the kernel so the viewport edge does not become a visible blur boundary.
        val radius = ((sqrt(4f * sigma * sigma + 1f) - 1f) / 2f).roundToInt().coerceAtLeast(1)
        val halo = radius * 3 + 1
        val workWidth = width + halo * 2
        val workHeight = height + halo * 2
        val raster = ImageBitmap(workWidth, workHeight)
        CanvasDrawScope().draw(
            Density(1f),
            LayoutDirection.Ltr,
            Canvas(raster),
            Size(workWidth.toFloat(), workHeight.toFloat()),
        ) {
            drawImage(
                source,
                dstOffset =
                    IntOffset(
                        (centerX - imageWidth / 2f).roundToInt() + halo,
                        (centerY - imageHeight / 2f).roundToInt() + halo,
                    ),
                dstSize = IntSize(imageWidth, imageHeight),
                colorFilter = filter,
                filterQuality = FilterQuality.Medium,
            )
        }
        val pixels = IntArray(workWidth * workHeight)
        raster.readPixels(pixels)
        for (i in pixels.indices) {
            val c = pixels[i]
            val a = c ushr 24
            pixels[i] =
                (a shl 24) or
                    (((c ushr 16 and 255) * a / 255) shl 16) or
                    (((c ushr 8 and 255) * a / 255) shl 8) or
                    ((c and 255) * a / 255)
        }
        val scratch = IntArray(pixels.size)
        repeat(3) {
            currentCoroutineContext().ensureActive()
            boxBlur(pixels, scratch, workWidth, workHeight, radius, true)
            boxBlur(scratch, pixels, workWidth, workHeight, radius, false)
        }
        for (y in 0 until height) for (x in 0 until width) {
            val src = pixels[(y + halo) * workWidth + x + halo]
            val index = y * width + x
            val dst = result[index]
            val inverseAlpha = 255 - (src ushr 24)
            fun channel(shift: Int) =
                (src ushr shift and 255) + (dst ushr shift and 255) * inverseAlpha / 255
            result[index] =
                0xff000000.toInt() or (channel(16) shl 16) or (channel(8) shl 8) or channel(0)
        }
    }
    return backgroundBitmap(result, width, height)
}

private fun saturationAndValueMatrix(
    saturation: Float,
    valueScale: Float,
): ColorMatrix {
    val inv = 1f - saturation
    val rw = 0.213f * inv
    val gw = 0.715f * inv
    val bw = 0.072f * inv

    return ColorMatrix(
        floatArrayOf(
            valueScale * (rw + saturation), valueScale * gw,                 valueScale * bw,                 0f, 0f,
            valueScale * rw,                 valueScale * (gw + saturation), valueScale * bw,                 0f, 0f,
            valueScale * rw,                 valueScale * gw,                 valueScale * (bw + saturation), 0f, 0f,
            0f,                              0f,                              0f,                              1f, 0f,
        )
    )
}

private fun lerp(start: Float, stop: Float, fraction: Float): Float {
    return start + (stop - start) * fraction
}

/** Sliding sums make each pass O(pixel count), independent of blur radius. */
private fun boxBlur(
    input: IntArray,
    output: IntArray,
    width: Int,
    height: Int,
    radius: Int,
    horizontal: Boolean,
) {
    val length = if (horizontal) width else height
    val lines = if (horizontal) height else width
    val stride = if (horizontal) 1 else width
    val divisor = radius * 2 + 1
    for (line in 0 until lines) {
        val base = if (horizontal) line * width else line
        var a = 0
        var r = 0
        var g = 0
        var b = 0
        fun add(position: Int, sign: Int) {
            val c = input[base + position.coerceIn(0, length - 1) * stride]
            a += (c ushr 24) * sign
            r += (c ushr 16 and 255) * sign
            g += (c ushr 8 and 255) * sign
            b += (c and 255) * sign
        }
        for (i in -radius..radius) add(i, 1)
        for (i in 0 until length) {
            output[base + i * stride] =
                ((a / divisor) shl 24) or
                    ((r / divisor) shl 16) or
                    ((g / divisor) shl 8) or
                    (b / divisor)
            add(i - radius, -1)
            add(i + radius + 1, 1)
        }
    }
}

internal expect fun backgroundBitmap(argb: IntArray, width: Int, height: Int): ImageBitmap
