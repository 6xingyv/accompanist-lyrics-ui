package com.mocharealm.accompanist.lyrics.ui.composable.lyrics

import android.content.Context
import android.graphics.Bitmap
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.remember
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.compose.ui.platform.LocalContext
import com.mocharealm.accompanist.lyrics.ui.renderer.RustSkiaLyricsView
import org.jetbrains.compose.resources.FontResource
import java.lang.ref.WeakReference
import kotlin.math.max

private const val NATIVE_ARTWORK_MAX_EDGE = 1024

internal class NativeArtwork(
    val pixels: IntArray,
    val width: Int,
    val height: Int,
)

/**
 * Prepares the bounded ARGB copy used by the native renderer. Clef calls this on
 * its cover-loading dispatcher, so opening the player never scales a bitmap or
 * executes getPixels on the UI thread.
 */
object NativeLyricsArtworkPrewarmer {
    private var cachedSource: WeakReference<ImageBitmap>? = null
    private var cachedArtwork: NativeArtwork? = null

    @Synchronized
    fun prewarm(source: ImageBitmap) {
        if (cachedSource?.get() === source) return
        cachedArtwork = source.toNativeArtwork()
        cachedSource = WeakReference(source)
    }

    @Synchronized
    internal fun get(source: ImageBitmap): NativeArtwork? =
        cachedArtwork?.takeIf { cachedSource?.get() === source }

    @Synchronized
    internal fun getOrPrepare(source: ImageBitmap): NativeArtwork {
        get(source)?.let { return it }
        return source.toNativeArtwork().also { artwork ->
            cachedArtwork = artwork
            cachedSource = WeakReference(source)
        }
    }

    private fun ImageBitmap.toNativeArtwork(): NativeArtwork {
        val bitmap = asAndroidBitmap()
        val longest = max(bitmap.width, bitmap.height)
        val scale = if (longest > NATIVE_ARTWORK_MAX_EDGE) {
            NATIVE_ARTWORK_MAX_EDGE.toFloat() / longest
        } else {
            1f
        }
        val width = max(1, (bitmap.width * scale).toInt())
        val height = max(1, (bitmap.height * scale).toInt())
        val scaled = if (scale < 1f) {
            Bitmap.createScaledBitmap(bitmap, width, height, true)
        } else {
            bitmap
        }
        val pixels = IntArray(scaled.width * scaled.height)
        scaled.getPixels(pixels, 0, scaled.width, 0, 0, scaled.width, scaled.height)
        return NativeArtwork(pixels, scaled.width, scaled.height)
    }
}

internal object NativeLyricsViewPool {
    private var available: RustSkiaLyricsView? = null
    private var leased = false

    @Synchronized
    fun prewarm(context: Context, fontBytes: ByteArray?, artwork: NativeArtwork?) {
        if (leased) return
        val appContext = context.applicationContext
        val view = available?.takeIf { it.context.applicationContext === appContext }
            ?: RustSkiaLyricsView(appContext).also { available = it }
        view.retainNativeEngineOnDetach = true
        view.configureFonts(fontBytes)
        view.setBackgroundArt(artwork?.pixels, artwork?.width ?: 0, artwork?.height ?: 0)
    }

    @Synchronized
    fun acquire(context: Context): RustSkiaLyricsView {
        val appContext = context.applicationContext
        val view = available?.takeIf { it.context.applicationContext === appContext }
            ?: RustSkiaLyricsView(appContext)
        available = null
        leased = true
        view.beginPlayerHostSession()
        view.retainNativeEngineOnDetach = true
        return view
    }

    @Synchronized
    fun recycle(view: RustSkiaLyricsView) {
        leased = false
        if (available == null) {
            view.retainNativeEngineOnDetach = true
            available = view
        } else {
            view.disposeNativeEngineWhenDetached()
        }
    }
}

/** Pre-creates and configures the exact native view later acquired by the player. */
@Composable
fun PrewarmNativeLyricsView(
    fontResource: FontResource? = null,
    backgroundArtwork: ImageBitmap? = null,
) {
    val context = LocalContext.current
    val fontBytes = rememberFontResourceBytes(fontResource)
    val artwork = remember(backgroundArtwork) {
        backgroundArtwork?.let(NativeLyricsArtworkPrewarmer::get)
    }
    LaunchedEffect(context, fontBytes, artwork) {
        // Let the currently-visible screen present first, then spend the following
        // main-thread slice on creating/configuring the retained TextureView.
        withFrameNanos { }
        NativeLyricsViewPool.prewarm(context, fontBytes, artwork)
    }
}
