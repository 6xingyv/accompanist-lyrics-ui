package com.mocharealm.accompanist.lyrics.text

import android.content.Context
import android.content.res.AssetFileDescriptor
import android.os.ParcelFileDescriptor
import android.view.Surface
import java.io.File
import java.nio.ByteBuffer

actual class NativeTextEngine actual constructor(
    actual val atlasWidth: Int,
    actual val atlasHeight: Int
) {

    companion object {
        init {
            System.loadLibrary("text_engine")
        }
    }

    private var handle: Long = nativeCreate(atlasWidth, atlasHeight)

    private var generationCounter: Int = 0
    actual internal val generation: Int
        get() = generationCounter

    actual fun configureFonts(config: NativeFontConfig): Boolean {
        ensureHandle()
        nativeInit(handle, atlasWidth, atlasHeight)

        val primaryLoaded = config.primary?.let(::loadPrimarySource) ?: false
        val fallbackLoads = config.fallbacks.count(::loadFallbackSource)
        // System fonts are pulled in lazily during shaping (NDK AFontMatcher),
        // so we don't eagerly load the whole platform collection here.
        generationCounter++
        return primaryLoaded || fallbackLoads > 0
    }

    actual fun processText(text: String, sizePx: Float, weight: Float): String {
        ensureHandle()
        return nativeProcessText(handle, text, sizePx, weight)
    }

    actual fun hasPendingUploads(): Boolean {
        return handle != 0L && nativeHasPendingUploads(handle)
    }

    actual internal fun getPendingUploadsJson(): String {
        return if (handle != 0L) nativeGetPendingUploads(handle) else "[]"
    }

    actual fun getAtlasSize(): String {
        return if (handle != 0L) nativeGetAtlasSize(handle) else """{"width":0,"height":0}"""
    }

    actual fun setLyricsScene(sceneJson: String): String {
        ensureHandle()
        return nativeSetLyricsScene(handle, sceneJson)
    }

    actual fun getLyricsRendererMetrics(): String {
        return if (handle != 0L) nativeGetLyricsRendererMetrics(handle) else "{}"
    }

    fun processTextDirect(text: String, sizePx: Float, weight: Float, buffer: ByteBuffer): Int {
        ensureHandle()
        return nativeProcessTextDirect(handle, text, sizePx, weight, buffer)
    }

    fun getPendingUploadsDirect(buffer: ByteBuffer): Int {
        return if (handle != 0L) nativeGetPendingUploadsDirect(handle, buffer) else -1
    }

    fun setRenderSurface(
        surface: Surface,
        surfaceWidth: Int,
        surfaceHeight: Int,
        frameWidth: Int,
        frameHeight: Int
    ): Boolean {
        ensureHandle()
        return nativeSetRenderSurface(handle, surface, surfaceWidth, surfaceHeight, frameWidth, frameHeight)
    }

    /**
     * Acquire the native window from [surface] on the CURRENT thread (must have a
     * valid JNIEnv, i.e. the main thread). Returns a window pointer (0 on
     * failure). Ownership transfers to the caller: hand it to
     * [setRenderSurfaceFromWindow] (which consumes it) or free it with
     * [releaseNativeWindow]. This lets the EGL setup run on the render thread
     * while the JNIEnv-dependent acquisition stays on the main thread.
     */
    fun acquireNativeWindow(surface: Surface): Long {
        return nativeAcquireNativeWindow(surface)
    }

    /** Free a window pointer that was never passed to [setRenderSurfaceFromWindow]. */
    fun releaseNativeWindow(windowPtr: Long) {
        if (windowPtr != 0L) nativeReleaseNativeWindow(windowPtr)
    }

    /**
     * Build the EGL renderer from a pre-acquired [windowPtr]. Safe to call off the
     * main thread (no JNIEnv needed). Consumes [windowPtr] on both success and
     * failure — the caller must not release it afterwards.
     */
    fun setRenderSurfaceFromWindow(windowPtr: Long, frameWidth: Int, frameHeight: Int): Boolean {
        ensureHandle()
        return nativeSetRenderSurfaceFromWindow(handle, windowPtr, frameWidth, frameHeight)
    }

    fun clearRenderSurface() {
        if (handle != 0L) {
            nativeClearRenderSurface(handle)
        }
    }

    fun renderLyricsFrameToSurface(currentTimeMs: Int): Int {
        ensureHandle()
        return nativeRenderLyricsFrameToSurface(handle, currentTimeMs)
    }

    fun beginLyricsScroll() {
        if (handle != 0L) nativeBeginLyricsScroll(handle)
    }

    fun scrollLyricsBy(deltaYPx: Float) {
        if (handle != 0L) nativeScrollLyricsBy(handle, deltaYPx)
    }

    fun endLyricsScroll(velocityYPx: Float) {
        if (handle != 0L) nativeEndLyricsScroll(handle, velocityYPx)
    }

    fun cancelLyricsScroll() {
        if (handle != 0L) nativeCancelLyricsScroll(handle)
    }

    fun resetLyricsScroll() {
        if (handle != 0L) nativeResetLyricsScroll(handle)
    }

    fun hitTestLyricsLine(x: Float, y: Float, currentTimeMs: Int): Int {
        return if (handle != 0L) nativeHitTestLyricsLine(handle, x, y, currentTimeMs) else -1
    }

    actual fun close() {
        val currentHandle = handle
        if (currentHandle != 0L) {
            nativeDestroy(currentHandle)
            handle = 0L
            generationCounter++
        }
    }

    private fun ensureHandle() {
        if (handle == 0L) {
            handle = nativeCreate(atlasWidth, atlasHeight)
            generationCounter++
        }
    }

    private fun loadPrimarySource(source: NativeFontSource): Boolean {
        loadAndroidDescriptorSource(source, ::nativeLoadFontFd, ::nativeLoadFontPath)?.let {
            return it
        }

        val bytes = source.bytes
        val path = source.path
        return when {
            bytes != null -> nativeLoadFont(handle, bytes, source.ttcIndex)
            !path.isNullOrBlank() -> nativeLoadFontPath(handle, path, source.ttcIndex)
            else -> false
        }
    }

    private fun loadFallbackSource(source: NativeFontSource): Boolean {
        loadAndroidDescriptorSource(source, ::nativeLoadFallbackFontFd, ::nativeLoadFallbackFontPath)?.let {
            return it
        }

        val bytes = source.bytes
        val path = source.path
        return when {
            bytes != null -> nativeLoadFallbackFont(handle, bytes, source.ttcIndex)
            !path.isNullOrBlank() -> nativeLoadFallbackFontPath(handle, path, source.ttcIndex)
            else -> false
        }
    }

    private fun loadAndroidDescriptorSource(
        source: NativeFontSource,
        fdLoader: (Long, Int, Long, Long, Int) -> Boolean,
        pathLoader: (Long, String, Int) -> Boolean
    ): Boolean? {
        val context = source.platformContext as? Context ?: return null
        val appContext = context.applicationContext

        source.resourceId?.let { resId ->
            try {
                appContext.resources.openRawResourceFd(resId)?.use { afd ->
                    return loadAssetFileDescriptor(source, afd, fdLoader)
                }
            } catch (_: Exception) {
                // Compressed resources cannot be opened as an fd range; stream them to cache below.
            }

            val extractedPath = copyResourceFontToCache(appContext, resId) ?: return false
            return pathLoader(handle, extractedPath, source.ttcIndex)
        }

        source.assetPath?.let { assetPath ->
            try {
                appContext.assets.openFd(assetPath).use { afd ->
                    return loadAssetFileDescriptor(source, afd, fdLoader)
                }
            } catch (_: Exception) {
                val extractedPath = copyAssetFontToCache(appContext, assetPath) ?: return false
                return pathLoader(handle, extractedPath, source.ttcIndex)
            }
        }

        return null
    }

    private fun loadAssetFileDescriptor(
        source: NativeFontSource,
        afd: AssetFileDescriptor,
        fdLoader: (Long, Int, Long, Long, Int) -> Boolean
    ): Boolean {
        return ParcelFileDescriptor.dup(afd.fileDescriptor).use { pfd ->
            fdLoader(
                handle,
                pfd.fd,
                afd.startOffset,
                afd.length,
                source.ttcIndex
            )
        }
    }

    private fun copyAssetFontToCache(context: Context, assetPath: String): String? {
        val outputFile = cacheFontFile(context, "asset-${assetPath.hashCode()}-${File(assetPath).name}")
        if (outputFile.exists() && outputFile.length() > 0L) return outputFile.absolutePath

        return try {
            outputFile.parentFile?.mkdirs()
            val tempFile = File(outputFile.parentFile, "${outputFile.name}.tmp")
            context.assets.open(assetPath).use { input ->
                tempFile.outputStream().use { output -> input.copyTo(output) }
            }
            if (tempFile.renameTo(outputFile) || tempFile.copyTo(outputFile, overwrite = true).exists()) {
                tempFile.delete()
                outputFile.absolutePath
            } else {
                null
            }
        } catch (_: Exception) {
            null
        }
    }

    private fun copyResourceFontToCache(context: Context, resourceId: Int): String? {
        val outputFile = cacheFontFile(context, "resource-$resourceId.font")
        if (outputFile.exists() && outputFile.length() > 0L) return outputFile.absolutePath

        return try {
            outputFile.parentFile?.mkdirs()
            val tempFile = File(outputFile.parentFile, "${outputFile.name}.tmp")
            context.resources.openRawResource(resourceId).use { input ->
                tempFile.outputStream().use { output -> input.copyTo(output) }
            }
            if (tempFile.renameTo(outputFile) || tempFile.copyTo(outputFile, overwrite = true).exists()) {
                tempFile.delete()
                outputFile.absolutePath
            } else {
                null
            }
        } catch (_: Exception) {
            null
        }
    }

    private fun cacheFontFile(context: Context, fileName: String): File {
        val safeName = fileName.replace(Regex("""[^A-Za-z0-9._-]"""), "_")
        return File(File(context.cacheDir, "lyrics-fonts"), safeName)
    }

    private external fun nativeCreate(atlasWidth: Int, atlasHeight: Int): Long
    private external fun nativeDestroy(handle: Long)
    private external fun nativeInit(handle: Long, atlasWidth: Int, atlasHeight: Int)
    private external fun nativeLoadFont(handle: Long, bytes: ByteArray, faceIndex: Int): Boolean
    private external fun nativeLoadFontPath(handle: Long, path: String, faceIndex: Int): Boolean
    private external fun nativeLoadFontFd(
        handle: Long,
        fd: Int,
        offset: Long,
        length: Long,
        faceIndex: Int
    ): Boolean

    private external fun nativeLoadFallbackFont(handle: Long, bytes: ByteArray, faceIndex: Int): Boolean
    private external fun nativeLoadFallbackFontPath(handle: Long, path: String, faceIndex: Int): Boolean
    private external fun nativeLoadFallbackFontFd(
        handle: Long,
        fd: Int,
        offset: Long,
        length: Long,
        faceIndex: Int
    ): Boolean

    private external fun nativeProcessText(handle: Long, text: String, sizePx: Float, weight: Float): String
    private external fun nativeHasPendingUploads(handle: Long): Boolean
    private external fun nativeGetPendingUploads(handle: Long): String
    private external fun nativeGetAtlasSize(handle: Long): String
    private external fun nativeSetLyricsScene(handle: Long, sceneJson: String): String
    private external fun nativeGetLyricsRendererMetrics(handle: Long): String
    private external fun nativeProcessTextDirect(
        handle: Long,
        text: String,
        sizePx: Float,
        weight: Float,
        buffer: ByteBuffer
    ): Int

    private external fun nativeGetPendingUploadsDirect(handle: Long, buffer: ByteBuffer): Int
    private external fun nativeSetRenderSurface(
        handle: Long,
        surface: Surface,
        surfaceWidth: Int,
        surfaceHeight: Int,
        frameWidth: Int,
        frameHeight: Int
    ): Boolean

    private external fun nativeAcquireNativeWindow(surface: Surface): Long
    private external fun nativeReleaseNativeWindow(windowPtr: Long)
    private external fun nativeSetRenderSurfaceFromWindow(
        handle: Long,
        windowPtr: Long,
        frameWidth: Int,
        frameHeight: Int
    ): Boolean

    private external fun nativeClearRenderSurface(handle: Long)

    private external fun nativeRenderLyricsFrameToSurface(
        handle: Long,
        currentTimeMs: Int
    ): Int

    private external fun nativeBeginLyricsScroll(handle: Long)
    private external fun nativeScrollLyricsBy(handle: Long, deltaYPx: Float)
    private external fun nativeEndLyricsScroll(handle: Long, velocityYPx: Float)
    private external fun nativeCancelLyricsScroll(handle: Long)
    private external fun nativeResetLyricsScroll(handle: Long)

    private external fun nativeHitTestLyricsLine(
        handle: Long,
        x: Float,
        y: Float,
        currentTimeMs: Int
    ): Int
}
