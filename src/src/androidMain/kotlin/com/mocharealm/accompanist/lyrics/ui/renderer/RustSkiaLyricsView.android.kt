package com.mocharealm.accompanist.lyrics.ui.renderer

import android.content.Context
import android.graphics.Color
import android.graphics.SurfaceTexture
import android.util.AttributeSet
import android.view.Surface
import android.view.GestureDetector
import android.view.MotionEvent
import android.view.TextureView
import android.view.VelocityTracker
import android.view.ViewConfiguration
import com.mocharealm.accompanist.lyrics.core.model.SyncedLyrics
import com.mocharealm.accompanist.lyrics.text.NativeFontConfig
import com.mocharealm.accompanist.lyrics.text.NativeFontSource
import com.mocharealm.accompanist.lyrics.text.NativeTextEngine
import com.mocharealm.accompanist.lyrics.ui.composable.lyrics.getFontSource
import kotlin.math.abs
import kotlin.math.max
import kotlin.math.roundToInt
import kotlin.math.sqrt

class RustSkiaLyricsView @JvmOverloads constructor(
    context: Context,
    attrs: AttributeSet? = null,
    defStyleAttr: Int = 0
) : TextureView(context, attrs, defStyleAttr), TextureView.SurfaceTextureListener {
    private val engine = NativeTextEngine(2048, 2048).apply {
        // System fonts are now pulled in natively (NDK) inside configureFonts, so
        // we only hand over the user's primary font here.
        configureFonts(
            NativeFontConfig(
                primary = getFontSource(null, context),
                fallbacks = emptyList()
            )
        )
    }

    private var lyrics: SyncedLyrics? = null
    private var currentTimeMs: Int = 0
    private var sceneDirty = true
    private var renderSurface: Surface? = null
    private var gpuSurfaceReady = false
    private var rendererStyle = defaultStyle()
    private var fontConfigKey = 0
    private var configuredFontBytes: ByteArray? = null
    private var engineClosed = false
    private var frameCallbackScheduled = false
    private var renderWidth = 0
    private var renderHeight = 0
    private var renderScale = 1f
    private val touchSlop = ViewConfiguration.get(context).scaledTouchSlop
    private val minFlingVelocity = ViewConfiguration.get(context).scaledMinimumFlingVelocity
    private val maxFlingVelocity = ViewConfiguration.get(context).scaledMaximumFlingVelocity
    private var velocityTracker: VelocityTracker? = null
    private var activePointerId = MotionEvent.INVALID_POINTER_ID
    private var isDragging = false
    private var downY = 0f
    private var lastTouchY = 0f
    private var onLineClicked: ((Int) -> Unit)? = null
    private var onLinePressed: ((Int) -> Unit)? = null

    private val gestureDetector = GestureDetector(context, object : GestureDetector.SimpleOnGestureListener() {
        override fun onDown(e: MotionEvent): Boolean = true

        override fun onSingleTapUp(e: MotionEvent): Boolean {
            val lineIndex = hitTestLine(e.x, e.y) ?: return false
            performClick()
            onLineClicked?.invoke(lineIndex)
            return true
        }

        override fun onLongPress(e: MotionEvent) {
            hitTestLine(e.x, e.y)?.let { lineIndex ->
                onLinePressed?.invoke(lineIndex)
            }
        }
    })

    init {
        surfaceTextureListener = this
        isOpaque = false
        isClickable = true
        isLongClickable = true
    }

    fun configureFonts(fontBytes: ByteArray?) {
        val key = fontBytes?.contentHashCode() ?: 0
        if (fontConfigKey == key && !engineClosed) return

        fontConfigKey = key
        configuredFontBytes = fontBytes
        applyCurrentFontConfig()
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
        val oldLocale = this.lyrics?.detectNativeLyricsLocale()
        this.lyrics = lyrics
        resetManualScroll()
        sceneDirty = true
        if (oldLocale != lyrics?.detectNativeLyricsLocale()) {
            applyCurrentFontConfig()
        } else {
            requestRender()
        }
    }

    fun setCurrentPosition(currentTimeMs: Int) {
        if (this.currentTimeMs == currentTimeMs) return
        this.currentTimeMs = currentTimeMs
        requestRender()
    }

    fun setRendererStyle(style: NativeLyricsRendererStyle) {
        if (rendererStyle == style && !engineClosed) return
        rendererStyle = style
        resetManualScroll()
        sceneDirty = true
        requestRender()
    }

    override fun onSurfaceTextureAvailable(surface: SurfaceTexture, width: Int, height: Int) {
        updateRenderTarget(width, height)
        bindRenderSurface(surface, width, height)
        sceneDirty = true
        renderFrame()
    }

    override fun onSurfaceTextureSizeChanged(surface: SurfaceTexture, width: Int, height: Int) {
        updateRenderTarget(width, height)
        bindRenderSurface(surface, width, height)
        sceneDirty = true
        renderFrame()
    }

    override fun onSurfaceTextureDestroyed(surface: SurfaceTexture): Boolean {
        releaseRenderSurface()
        return true
    }

    override fun onSurfaceTextureUpdated(surface: SurfaceTexture) {
    }

    private fun renderFrame() {
        frameCallbackScheduled = false
        if (!isAttachedToWindow || engineClosed) return
        val viewWidth = width
        val viewHeight = height
        if (!isAvailable || viewWidth <= 0 || viewHeight <= 0) return

        if (updateRenderTarget(viewWidth, viewHeight) && gpuSurfaceReady) {
            surfaceTexture?.let { bindRenderSurface(it, viewWidth, viewHeight) }
        }
        if (!ensureScene(viewWidth, viewHeight)) return
        if (!gpuSurfaceReady) return

        val result = engine.renderLyricsFrameToSurface(currentTimeMs)
        if (result < 0) {
            gpuSurfaceReady = false
            engine.clearRenderSurface()
            renderSurface?.release()
            renderSurface = null
            return
        }
        if (result > 0) {
            requestRender()
        }
    }

    private fun requestRender() {
        if (!isAttachedToWindow) return
        if (frameCallbackScheduled) return
        frameCallbackScheduled = true
        postOnAnimation {
            renderFrame()
        }
    }

    override fun onTouchEvent(event: MotionEvent): Boolean {
        if (lyrics == null) return super.onTouchEvent(event)

        when (event.actionMasked) {
            MotionEvent.ACTION_DOWN -> {
                parent?.requestDisallowInterceptTouchEvent(true)
                isDragging = false
                activePointerId = event.getPointerId(0)
                downY = event.y
                lastTouchY = event.y
                velocityTracker?.recycle()
                velocityTracker = VelocityTracker.obtain().also { it.addMovement(event) }
                gestureDetector.onTouchEvent(event)
                return true
            }

            MotionEvent.ACTION_POINTER_UP -> {
                handlePointerUp(event)
                velocityTracker?.addMovement(event)
                return true
            }

            MotionEvent.ACTION_MOVE -> {
                val pointerIndex = event.findPointerIndex(activePointerId)
                if (pointerIndex < 0) return false
                velocityTracker?.addMovement(event)
                val y = event.getY(pointerIndex)
                if (!isDragging && abs(y - downY) > touchSlop) {
                    isDragging = true
                    lastTouchY = y
                    engine.beginLyricsScroll()
                    cancelTapDetection(event)
                    return true
                }

                if (isDragging) {
                    val dy = y - lastTouchY
                    if (dy != 0f) {
                        engine.scrollLyricsBy(-dy * renderScale)
                        requestRender()
                    }
                    lastTouchY = y
                    return true
                }

                return gestureDetector.onTouchEvent(event) || super.onTouchEvent(event)
            }

            MotionEvent.ACTION_UP -> {
                val wasDragging = isDragging
                velocityTracker?.addMovement(event)
                if (wasDragging) {
                    finishManualDrag()
                } else {
                    recycleTouchState()
                    parent?.requestDisallowInterceptTouchEvent(false)
                    return gestureDetector.onTouchEvent(event) || super.onTouchEvent(event)
                }
                parent?.requestDisallowInterceptTouchEvent(false)
                return true
            }

            MotionEvent.ACTION_CANCEL -> {
                engine.cancelLyricsScroll()
                recycleTouchState()
                parent?.requestDisallowInterceptTouchEvent(false)
                requestRender()
                return true
            }
        }

        return gestureDetector.onTouchEvent(event) || super.onTouchEvent(event)
    }

    override fun performClick(): Boolean {
        super.performClick()
        return true
    }

    override fun onDetachedFromWindow() {
        super.onDetachedFromWindow()
        releaseRenderSurface()
        engine.close()
        engineClosed = true
    }

    override fun onAttachedToWindow() {
        super.onAttachedToWindow()
        if (engineClosed) {
            applyCurrentFontConfig()
        }
        requestRender()
    }

    private fun bindRenderSurface(surfaceTexture: SurfaceTexture, width: Int, height: Int) {
        releaseRenderSurface()
        if (width <= 0 || height <= 0 || engineClosed) return

        updateRenderTarget(width, height)
        val frameWidth = renderWidth.takeIf { it > 0 } ?: width
        val frameHeight = renderHeight.takeIf { it > 0 } ?: height
        surfaceTexture.setDefaultBufferSize(frameWidth, frameHeight)

        val surface = Surface(surfaceTexture)
        requestHighestRefreshRate(surface)
        val enabled = engine.setRenderSurface(surface, frameWidth, frameHeight, frameWidth, frameHeight)
        if (enabled) {
            renderSurface = surface
            gpuSurfaceReady = true
        } else {
            surface.release()
            renderSurface = null
            gpuSurfaceReady = false
        }
    }

    /**
     * The lyrics animate continuously, so ask the system to run this surface at
     * the display's highest refresh rate (e.g. 120Hz) instead of the default
     * 60Hz. Hint only — the platform decides; it's a no-op on single-mode (60Hz)
     * displays and on API < 30.
     */
    private fun requestHighestRefreshRate(surface: Surface) {
        if (android.os.Build.VERSION.SDK_INT < android.os.Build.VERSION_CODES.R) return
        val maxRate = display?.supportedModes?.maxOfOrNull { it.refreshRate } ?: return
        if (maxRate > 0f) {
            runCatching {
                surface.setFrameRate(maxRate, Surface.FRAME_RATE_COMPATIBILITY_DEFAULT)
            }
        }
    }

    private fun releaseRenderSurface() {
        frameCallbackScheduled = false
        gpuSurfaceReady = false
        engine.clearRenderSurface()
        renderSurface?.release()
        renderSurface = null
    }

    private fun ensureScene(width: Int, height: Int): Boolean {
        val sceneLyrics = lyrics ?: return false
        updateRenderTarget(width, height)
        val sceneWidth = renderWidth.takeIf { it > 0 } ?: width
        val sceneHeight = renderHeight.takeIf { it > 0 } ?: height
        if (sceneDirty) {
            engine.setLyricsScene(
                sceneLyrics.toNativeLyricsSceneJson(
                    sceneWidth,
                    sceneHeight,
                    rendererStyle.scaled(renderScale)
                )
            )
            sceneDirty = false
        }
        return true
    }

    private fun hitTestLine(x: Float, y: Float): Int? {
        val sceneLyrics = lyrics ?: return null
        val width = width
        val height = height
        if (width <= 0 || height <= 0) return null
        if (!ensureScene(width, height)) return null

        val lineIndex = engine.hitTestLyricsLine(x * renderScale, y * renderScale, currentTimeMs)
        return lineIndex.takeIf { it in sceneLyrics.lines.indices }
    }

    private fun applyCurrentFontConfig() {
        val fontBytes = configuredFontBytes
        // Only the user's primary font; system fonts come from the NDK pool that
        // configureFonts loads, and cosmic-text falls back by the scene's locale.
        engine.configureFonts(
            NativeFontConfig(
                primary = fontBytes?.let { NativeFontSource(bytes = it) } ?: getFontSource(null, context),
                fallbacks = emptyList()
            )
        )
        engineClosed = false
        resetManualScroll()
        if (isAvailable && width > 0 && height > 0) {
            surfaceTexture?.let { surfaceTexture ->
                bindRenderSurface(surfaceTexture, width, height)
            }
        }
        sceneDirty = true
        requestRender()
    }

    private fun resetManualScroll() {
        engine.resetLyricsScroll()
    }

    private fun finishManualDrag() {
        val tracker = velocityTracker
        tracker?.computeCurrentVelocity(1000, maxFlingVelocity.toFloat())
        val yVelocity = tracker?.getYVelocity(activePointerId) ?: 0f
        val nativeVelocity = if (abs(yVelocity) >= minFlingVelocity) {
            -yVelocity * renderScale
        } else {
            0f
        }
        engine.endLyricsScroll(nativeVelocity)
        recycleTouchState()
        requestRender()
    }

    private fun handlePointerUp(event: MotionEvent) {
        val pointerIndex = event.actionIndex
        if (event.getPointerId(pointerIndex) != activePointerId) return

        val nextPointerIndex = if (pointerIndex == 0) 1 else 0
        if (nextPointerIndex >= event.pointerCount) {
            activePointerId = MotionEvent.INVALID_POINTER_ID
            return
        }

        activePointerId = event.getPointerId(nextPointerIndex)
        downY = event.getY(nextPointerIndex)
        lastTouchY = downY
        velocityTracker?.clear()
    }

    private fun cancelTapDetection(event: MotionEvent) {
        val cancelEvent = MotionEvent.obtain(event)
        cancelEvent.action = MotionEvent.ACTION_CANCEL
        gestureDetector.onTouchEvent(cancelEvent)
        cancelEvent.recycle()
    }

    private fun recycleTouchState() {
        velocityTracker?.recycle()
        velocityTracker = null
        activePointerId = MotionEvent.INVALID_POINTER_ID
        isDragging = false
    }

    private fun updateRenderTarget(width: Int, height: Int): Boolean {
        if (width <= 0 || height <= 0) return false

        val pixels = width.toFloat() * height.toFloat()
        val scale = if (pixels > MAX_RENDER_PIXELS) {
            max(MIN_RENDER_SCALE, sqrt(MAX_RENDER_PIXELS / pixels))
        } else {
            1f
        }
        val nextWidth = max(1, (width * scale).roundToInt())
        val nextHeight = max(1, (height * scale).roundToInt())
        if (nextWidth == renderWidth && nextHeight == renderHeight && scale == renderScale) {
            return false
        }

        renderWidth = nextWidth
        renderHeight = nextHeight
        renderScale = scale
        sceneDirty = true
        return true
    }

    private fun defaultStyle(): NativeLyricsRendererStyle {
        val density = resources.displayMetrics.density
        val scaledDensity = density * resources.configuration.fontScale
        return NativeLyricsRendererStyle(
            normalFontSizePx = 34f * scaledDensity,
            normalLineHeightPx = 42f * scaledDensity,
            accompanimentFontSizePx = 20f * scaledDensity,
            accompanimentLineHeightPx = 26f * scaledDensity,
            translationFontSizePx = 16f * scaledDensity,
            translationLineHeightPx = 21f * scaledDensity,
            // Main + accompaniment lines are bold; translation/phonetic are regular.
            normalFontWeight = 700,
            accompanimentFontWeight = 700,
            translationFontWeight = 400,
            phoneticFontWeight = 400,
            paddingXPx = 16f * density,
            paddingYPx = 8f * density,
            keepAlivePx = 120f * density,
            textColorArgb = Color.WHITE,
        )
    }

    private fun NativeLyricsRendererStyle.scaled(scale: Float): NativeLyricsRendererStyle {
        if (scale == 1f) return this
        return copy(
            normalFontSizePx = normalFontSizePx * scale,
            normalLineHeightPx = normalLineHeightPx * scale,
            accompanimentFontSizePx = accompanimentFontSizePx * scale,
            accompanimentLineHeightPx = accompanimentLineHeightPx * scale,
            translationFontSizePx = translationFontSizePx * scale,
            translationLineHeightPx = translationLineHeightPx * scale,
            phoneticFontSizePx = phoneticFontSizePx * scale,
            phoneticLineHeightPx = phoneticLineHeightPx * scale,
            phoneticGapPx = phoneticGapPx * scale,
            paddingXPx = paddingXPx * scale,
            paddingYPx = paddingYPx * scale,
            keepAlivePx = keepAlivePx * scale,
            blurDelta = blurDelta * scale,
            breathingDotsSizePx = breathingDotsSizePx * scale,
            breathingDotsMarginPx = breathingDotsMarginPx * scale
        )
    }

    private companion object {
        const val MAX_RENDER_PIXELS = 2_200_000f
        const val MIN_RENDER_SCALE = 0.7f
    }
}
