package com.mocharealm.accompanist.sample.ui.composable.background

import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.toComposeImageBitmap
import org.jetbrains.skia.Image
import org.jetbrains.skia.ImageInfo

internal actual fun backgroundBitmap(argb: IntArray, width: Int, height: Int): ImageBitmap {
    // Output pixels are opaque, so no additional premultiplication is needed.
    val bytes = ByteArray(argb.size * 4)
    for (i in argb.indices) for (channel in 0..3) bytes[i * 4 + channel] =
        (argb[i] ushr (channel * 8)).toByte()
    return Image.makeRaster(ImageInfo.makeN32Premul(width, height), bytes, width * 4)
        .toComposeImageBitmap()
}
