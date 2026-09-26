package com.mocharealm.accompanist.sample.ui.screen.player

import android.graphics.Bitmap
import android.graphics.BitmapFactory
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.unit.IntSize
import com.mocharealm.accompanist.sample.ui.composable.artworkSize
import androidx.core.graphics.scale

/** Bound the cover texture used by the player; the encoded original stays in media metadata. */
internal fun decodePlayerArtwork(data: ByteArray, viewport: IntSize): ImageBitmap? {
    val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
    BitmapFactory.decodeByteArray(data, 0, data.size, bounds)
    if (bounds.outWidth <= 0 || bounds.outHeight <= 0) return null
    val target = artworkSize(IntSize(bounds.outWidth, bounds.outHeight), viewport)
    var sample = 1
    while (
        bounds.outWidth / (sample * 2) >= target.width &&
            bounds.outHeight / (sample * 2) >= target.height
    ) sample *= 2
    val decoded =
        BitmapFactory.decodeByteArray(
            data,
            0,
            data.size,
            BitmapFactory.Options().apply { inSampleSize = sample },
        ) ?: return null
    val result =
        if (decoded.width != target.width || decoded.height != target.height) {
            decoded.scale(target.width, target.height).also {
                if (it !== decoded) decoded.recycle()
            }
        } else decoded
    // Background preparation reads CPU pixels; only an actual display consumer requests upload.
    return result.asImageBitmap()
}
