package com.mocharealm.accompanist.sample.ui.screen.player

import android.graphics.Bitmap
import androidx.compose.foundation.Image
import androidx.compose.foundation.layout.Box
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.unit.IntSize
import com.mocharealm.accompanist.sample.ui.composable.artworkSize
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.withContext
import kotlin.time.Duration.Companion.milliseconds
import androidx.core.graphics.scale

/** Layout size, not playback state or source resolution, owns the display texture. */
@Composable
internal fun PlayerArtwork(data: ByteArray?, fallback: ImageBitmap, modifier: Modifier = Modifier) {
    var viewport by remember { mutableStateOf(IntSize.Zero) }
    val fallbackKey = if (data == null) fallback else null
    var bitmap by remember(data, fallbackKey) { mutableStateOf<ImageBitmap?>(null) }
    LaunchedEffect(data, fallbackKey, viewport) {
        if (viewport.width <= 0 || viewport.height <= 0) return@LaunchedEffect
        // Coalesce window resizing; keep the current texture visible until the replacement is
        // ready.
        if (bitmap != null) delay(100.milliseconds)
        bitmap =
            withContext(Dispatchers.Default) {
                val decoded = data?.let { decodePlayerArtwork(it, viewport) }
                val result =
                    decoded
                        ?: run {
                            val source = fallback.asAndroidBitmap()
                            val target = artworkSize(IntSize(source.width, source.height), viewport)
                            source.scale(target.width, target.height)
                                .asImageBitmap()
                        }
                result.asAndroidBitmap().prepareToDraw()
                result
            }
    }
    Box(modifier.onSizeChanged { viewport = it }) {
        bitmap?.let { Image(it, null, Modifier.matchParentSize()) }
    }
}
