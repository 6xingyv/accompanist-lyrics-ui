package com.mocharealm.accompanist.lyrics.text

import java.nio.ByteBuffer
import kotlin.io.path.createTempFile

actual class NativeTextEngine actual constructor(
    actual val atlasWidth: Int,
    actual val atlasHeight: Int
) {

    companion object {
        init {
            NativeTextEngineLibrary.load()
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

    fun renderLyricsFrameDirect(currentTimeMs: Int, buffer: ByteBuffer): Int {
        ensureHandle()
        return nativeRenderLyricsFrameDirect(handle, currentTimeMs, buffer)
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
        val bytes = source.bytes
        val path = source.path
        return when {
            bytes != null -> nativeLoadFont(handle, bytes, source.ttcIndex)
            !path.isNullOrBlank() -> nativeLoadFontPath(handle, path, source.ttcIndex)
            else -> false
        }
    }

    private fun loadFallbackSource(source: NativeFontSource): Boolean {
        val bytes = source.bytes
        val path = source.path
        return when {
            bytes != null -> nativeLoadFallbackFont(handle, bytes, source.ttcIndex)
            !path.isNullOrBlank() -> nativeLoadFallbackFontPath(handle, path, source.ttcIndex)
            else -> false
        }
    }

    private external fun nativeCreate(atlasWidth: Int, atlasHeight: Int): Long
    private external fun nativeDestroy(handle: Long)
    private external fun nativeInit(handle: Long, atlasWidth: Int, atlasHeight: Int)
    private external fun nativeLoadFont(handle: Long, bytes: ByteArray, faceIndex: Int): Boolean
    private external fun nativeLoadFontPath(handle: Long, path: String, faceIndex: Int): Boolean
    private external fun nativeLoadFallbackFont(handle: Long, bytes: ByteArray, faceIndex: Int): Boolean
    private external fun nativeLoadFallbackFontPath(handle: Long, path: String, faceIndex: Int): Boolean
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
    private external fun nativeRenderLyricsFrameDirect(
        handle: Long,
        currentTimeMs: Int,
        buffer: ByteBuffer
    ): Int

    private external fun nativeHitTestLyricsLine(
        handle: Long,
        x: Float,
        y: Float,
        currentTimeMs: Int
    ): Int
}

private object NativeTextEngineLibrary {
    @Volatile
    private var loaded = false

    fun load() {
        if (loaded) return
        synchronized(this) {
            if (loaded) return

            val mappedName = System.mapLibraryName("text_engine")
            val osArch = "${osId()}-${archId()}"
            val candidates = listOf(
                "natives/$osArch/$mappedName",
                "natives/$mappedName",
            )

            for (resourcePath in candidates) {
                val stream = Thread.currentThread().contextClassLoader
                    ?.getResourceAsStream(resourcePath)
                    ?: NativeTextEngineLibrary::class.java.classLoader.getResourceAsStream(resourcePath)
                if (stream != null) {
                    val suffix = mappedName.substringAfterLast('.', "")
                        .takeIf { it.isNotEmpty() }
                        ?.let { ".$it" }
                        ?: ".bin"
                    val tempFile = createTempFile("text_engine_", suffix).toFile()
                    stream.use { input ->
                        tempFile.outputStream().use { output -> input.copyTo(output) }
                    }
                    tempFile.deleteOnExit()
                    System.load(tempFile.absolutePath)
                    loaded = true
                    return
                }
            }

            System.loadLibrary("text_engine")
            loaded = true
        }
    }

    private fun osId(): String {
        val os = System.getProperty("os.name").lowercase()
        return when {
            os.contains("win") -> "windows"
            os.contains("mac") || os.contains("darwin") -> "macos"
            else -> "linux"
        }
    }

    private fun archId(): String {
        val arch = System.getProperty("os.arch").lowercase()
        return when {
            arch == "x86_64" || arch == "amd64" -> "x86_64"
            arch == "aarch64" || arch == "arm64" -> "arm64"
            else -> arch
        }
    }
}
