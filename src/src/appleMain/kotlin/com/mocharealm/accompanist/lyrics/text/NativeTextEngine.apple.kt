package com.mocharealm.accompanist.lyrics.text

actual class NativeTextEngine actual constructor(
    actual val atlasWidth: Int,
    actual val atlasHeight: Int
) {
    private var generationCounter: Int = 0
    actual internal val generation: Int
        get() = generationCounter

    actual fun configureFonts(config: NativeFontConfig): Boolean {
        generationCounter++
        return false
    }

    actual fun processText(text: String, sizePx: Float, weight: Float): String {
        return "{}"
    }

    actual fun hasPendingUploads(): Boolean = false

    actual internal fun getPendingUploadsJson(): String = "[]"

    actual fun getAtlasSize(): String {
        return """{"width":$atlasWidth,"height":$atlasHeight}"""
    }

    actual fun setLyricsScene(sceneJson: String): String = "{}"

    actual fun getLyricsRendererMetrics(): String = "{}"

    actual fun close() {
        generationCounter++
    }
}
