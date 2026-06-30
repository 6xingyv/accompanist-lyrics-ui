package com.mocharealm.accompanist.lyrics.ui.renderer

import com.mocharealm.accompanist.lyrics.core.model.SyncedLyrics
import com.mocharealm.accompanist.lyrics.text.NativeFontConfig
import com.mocharealm.accompanist.lyrics.text.NativeFontSource
import com.mocharealm.accompanist.lyrics.text.NativeTextEngine
import com.mocharealm.accompanist.lyrics.ui.composable.lyrics.getFontSource
import com.mocharealm.accompanist.lyrics.ui.composable.lyrics.getSystemFallbackFontSources
import java.awt.Color
import java.awt.Graphics
import java.awt.Graphics2D
import java.awt.event.MouseAdapter
import java.awt.event.MouseEvent
import java.awt.image.BufferedImage
import java.awt.image.DataBufferInt
import java.nio.ByteBuffer
import java.nio.ByteOrder
import javax.swing.JPanel
import javax.swing.SwingUtilities

class RustSkiaLyricsPanel : JPanel() {
    private val engine = NativeTextEngine(2048, 2048).apply {
        configureFonts(
            NativeFontConfig(
                primary = getFontSource(null, null),
                fallbacks = getSystemFallbackFontSources(null)
            )
        )
    }

    private var lyrics: SyncedLyrics? = null
    private var currentTimeMs: Int = 0
    private var sceneDirty = true
    private var pixelBuffer: ByteBuffer? = null
    private var image: BufferedImage? = null
    private var rendererStyle = defaultStyle()
    private var fontConfigKey = 0
    private var onLineClicked: ((Int) -> Unit)? = null
    private var onLinePressed: ((Int) -> Unit)? = null

    init {
        isOpaque = false
        addMouseListener(object : MouseAdapter() {
            override fun mouseClicked(event: MouseEvent) {
                if (SwingUtilities.isLeftMouseButton(event) && event.clickCount == 1) {
                    hitTestLine(event.x.toFloat(), event.y.toFloat())?.let { lineIndex ->
                        onLineClicked?.invoke(lineIndex)
                    }
                }
            }

            override fun mousePressed(event: MouseEvent) {
                handleContextPress(event)
            }

            override fun mouseReleased(event: MouseEvent) {
                handleContextPress(event)
            }
        })
    }

    fun configureFonts(fontBytes: ByteArray?) {
        val key = fontBytes?.contentHashCode() ?: 0
        if (fontConfigKey == key) return

        fontConfigKey = key
        engine.configureFonts(
            NativeFontConfig(
                primary = fontBytes?.let { NativeFontSource(bytes = it) } ?: getFontSource(null, null),
                fallbacks = getSystemFallbackFontSources(null)
            )
        )
        sceneDirty = true
        repaint()
    }

    fun setLineInteractionCallbacks(
        onLineClicked: ((Int) -> Unit)?,
        onLinePressed: ((Int) -> Unit)?
    ) {
        this.onLineClicked = onLineClicked
        this.onLinePressed = onLinePressed
    }

    fun setLyrics(lyrics: SyncedLyrics?) {
        if (this.lyrics === lyrics) return
        this.lyrics = lyrics
        sceneDirty = true
        repaint()
    }

    fun setCurrentPosition(currentTimeMs: Int) {
        if (this.currentTimeMs == currentTimeMs) return
        this.currentTimeMs = currentTimeMs
        repaint()
    }

    fun setRendererStyle(style: NativeLyricsRendererStyle) {
        if (rendererStyle == style) return
        rendererStyle = style
        sceneDirty = true
        repaint()
    }

    override fun paintComponent(graphics: Graphics) {
        super.paintComponent(graphics)
        val width = width
        val height = height
        if (width <= 0 || height <= 0) return

        ensureBuffers(width, height)
        if (!ensureScene(width, height)) return

        val buffer = pixelBuffer ?: return
        val target = image ?: return
        buffer.clear()
        val result = engine.renderLyricsFrameDirect(currentTimeMs, buffer)
        if (result == 0) {
            copyRgbaToBufferedImage(buffer, target)
            (graphics as Graphics2D).drawImage(target, 0, 0, null)
        }
    }

    override fun removeNotify() {
        super.removeNotify()
        engine.close()
    }

    private fun ensureBuffers(width: Int, height: Int) {
        val current = image
        if (current != null && current.width == width && current.height == height && pixelBuffer != null) {
            return
        }
        pixelBuffer = ByteBuffer.allocateDirect(width * height * 4).order(ByteOrder.nativeOrder())
        image = BufferedImage(width, height, BufferedImage.TYPE_INT_ARGB)
        sceneDirty = true
    }

    private fun ensureScene(width: Int, height: Int): Boolean {
        val sceneLyrics = lyrics ?: return false
        if (sceneDirty) {
            engine.setLyricsScene(sceneLyrics.toNativeLyricsSceneJson(width, height, rendererStyle))
            sceneDirty = false
        }
        return true
    }

    private fun hitTestLine(x: Float, y: Float): Int? {
        val sceneLyrics = lyrics ?: return null
        val width = width
        val height = height
        if (width <= 0 || height <= 0) return null
        ensureBuffers(width, height)
        if (!ensureScene(width, height)) return null

        val lineIndex = engine.hitTestLyricsLine(x, y, currentTimeMs)
        return lineIndex.takeIf { it in sceneLyrics.lines.indices }
    }

    private fun handleContextPress(event: MouseEvent) {
        if (!event.isPopupTrigger && !SwingUtilities.isRightMouseButton(event)) return
        hitTestLine(event.x.toFloat(), event.y.toFloat())?.let { lineIndex ->
            onLinePressed?.invoke(lineIndex)
        }
    }

    private fun copyRgbaToBufferedImage(buffer: ByteBuffer, image: BufferedImage) {
        buffer.rewind()
        val pixels = (image.raster.dataBuffer as DataBufferInt).data
        var index = 0
        while (index < pixels.size && buffer.remaining() >= 4) {
            val r = buffer.get().toInt() and 0xff
            val g = buffer.get().toInt() and 0xff
            val b = buffer.get().toInt() and 0xff
            val a = buffer.get().toInt() and 0xff
            pixels[index] = (a shl 24) or (r shl 16) or (g shl 8) or b
            index++
        }
    }

    private fun defaultStyle(): NativeLyricsRendererStyle {
        return NativeLyricsRendererStyle(
            normalFontSizePx = 34f,
            normalLineHeightPx = 42f,
            accompanimentFontSizePx = 20f,
            accompanimentLineHeightPx = 26f,
            translationFontSizePx = 16f,
            translationLineHeightPx = 21f,
            paddingXPx = 16f,
            paddingYPx = 8f,
            keepAlivePx = 120f,
            textColorArgb = Color.WHITE.rgb,
        )
    }
}
