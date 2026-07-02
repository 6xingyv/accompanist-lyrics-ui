package com.mocharealm.accompanist.lyrics.ui.renderer

import android.content.Context
import android.graphics.Color
import android.graphics.SurfaceTexture
import android.os.Handler
import android.os.HandlerThread
import android.util.AttributeSet
import android.view.Choreographer
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
import androidx.compose.ui.unit.Density
import com.mocharealm.accompanist.lyrics.ui.composable.lyrics.KaraokeLyricsConfig
import com.mocharealm.accompanist.lyrics.ui.composable.lyrics.getFontSource
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicInteger
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

    // Playback position is written on the main thread and read on the render
    // thread every frame, so it must be volatile. It is the ONLY per-frame value
    // handed across threads — everything else the render thread needs lives behind
    // the engine's own mutex.
    @Volatile
    private var currentTimeMs: Int = 0
    private var sceneDirty = true
    private var renderSurface: Surface? = null
    // Initial style is just the default config mapped at this view's density; the
    // host overwrites it via setStyle before the first frame when one is supplied.
    // No separate hand-written default to drift from the config.
    private var sceneStyle = KaraokeLyricsConfig().toSceneStyle(
        Density(resources.displayMetrics.density, resources.configuration.fontScale)
    )
    private var configuredFontBytes: ByteArray? = null
    @Volatile
    private var engineClosed = false

    // --- Dedicated render thread ---------------------------------------------
    // The EGL context is created, used (draw + blocking eglSwapBuffers), and
    // destroyed exclusively on this thread, so the main/UI thread never blocks on
    // the vsync swap and Compose stops competing with rendering.
    private var renderThread: HandlerThread? = null
    private var renderHandler: Handler? = null
    // Obtained on the render thread (Choreographer is thread-local). Written on
    // the render thread; nulled on the main thread only after the thread is joined.
    @Volatile
    private var renderChoreographer: Choreographer? = null
    // Render-thread-only state (never touched from the main thread).
    private var surfaceReady = false
    private var renderScheduled = false
    // Coalesces main-thread wake-ups so a burst of setCurrentPosition / touch
    // events posts at most one Runnable to the render thread.
    private val wakePending = AtomicBoolean(false)
    private val frameCallback = Choreographer.FrameCallback { doFrame() }
    private val wakeRunnable = Runnable {
        wakePending.set(false)
        scheduleFrame()
    }
    // Drag deltas accumulate here (as float bits) from ACTION_MOVE on the main
    // thread WITHOUT taking the engine lock, and are applied on the render thread
    // inside doFrame. This keeps fast drags off the engine mutex, which the render
    // thread holds for a whole frame — the old per-MOVE engine.scrollLyricsBy could
    // block the UI thread ~a frame each move and stutter the drag.
    private val pendingScrollDeltaBits = AtomicInteger(0)

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
        // `fontBytes` comes from rememberFontResourceBytes, which returns a *stable*
        // instance across recompositions, so an identity check dedupes in O(1). The
        // old `fontBytes.contentHashCode()` hashed the entire (multi-MB) font on
        // every recomposition — i.e. ~60×/s during playback — which was the source
        // of the heavy stutter whenever a FontResource was set. Reloading the font
        // (below) rebuilds the whole scene, so it must only happen when the font
        // actually changes.
        if (configuredFontBytes === fontBytes && !engineClosed) return

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
            rebuildSceneAndRender()
        }
    }

    fun setCurrentPosition(currentTimeMs: Int) {
        if (this.currentTimeMs == currentTimeMs) return
        this.currentTimeMs = currentTimeMs
        requestRender()
    }

    internal fun setStyle(style: SceneStyle) {
        if (sceneStyle == style && !engineClosed) return
        sceneStyle = style
        resetManualScroll()
        sceneDirty = true
        rebuildSceneAndRender()
    }

    /**
     * Rebuild the (dirty) scene on the main thread — infrequent, so keeping it here
     * avoids threading the lyrics/style snapshot onto the render thread — then wake
     * the render thread to draw it.
     */
    private fun rebuildSceneAndRender() {
        if (width > 0 && height > 0) ensureScene(width, height)
        requestRender()
    }

    override fun onSurfaceTextureAvailable(surface: SurfaceTexture, width: Int, height: Int) {
        updateRenderTarget(width, height)
        sceneDirty = true
        bindRenderSurface(surface, width, height)
    }

    override fun onSurfaceTextureSizeChanged(surface: SurfaceTexture, width: Int, height: Int) {
        updateRenderTarget(width, height)
        sceneDirty = true
        bindRenderSurface(surface, width, height)
    }

    override fun onSurfaceTextureDestroyed(surface: SurfaceTexture): Boolean {
        // Blocks until the render thread has torn down EGL and stopped touching the
        // surface, so returning true (which frees the SurfaceTexture) is safe.
        releaseRenderSurface()
        return true
    }

    override fun onSurfaceTextureUpdated(surface: SurfaceTexture) {
    }

    /**
     * Wake the render loop. Runs on the main thread and only hops one coalesced
     * Runnable over to the render thread, where [scheduleFrame] posts a
     * vsync-paced frame. No-op until the render thread exists (the surface
     * callbacks create it).
     */
    private fun requestRender() {
        val handler = renderHandler ?: return
        if (wakePending.compareAndSet(false, true)) {
            handler.post(wakeRunnable)
        }
    }

    /** Render thread only: schedule one frame on the next vsync (deduped). */
    private fun scheduleFrame() {
        if (renderScheduled || !surfaceReady) return
        renderScheduled = true
        renderChoreographer?.postFrameCallback(frameCallback)
    }

    /**
     * Render thread only: draw + present one frame and keep the loop alive while
     * the engine reports animation/scroll activity (return > 0). When it returns
     * 0 the loop parks until the main thread calls [requestRender] again.
     */
    private fun doFrame() {
        renderScheduled = false
        if (!surfaceReady) return
        applyPendingScrollOnRenderThread()
        // Arm the next vsync callback BEFORE the blocking present. eglSwapBuffers
        // (swapInterval 1) blocks the render thread until the next vsync, so posting
        // the callback AFTER it would register past the vsync boundary and only fire
        // a vsync later — halving the effective rate (present once per two refreshes)
        // while the GPU sits idle in the swap. Registering it first lets Choreographer
        // stamp it against the current vsync, so it fires on the next one → one present
        // per refresh. If this frame turns out idle/lost, the callback is cancelled
        // below so the loop still parks.
        scheduleFrame()
        val result = engine.renderLyricsFrameToSurface(currentTimeMs)
        if (result < 0) {
            // Surface lost — drop EGL. The Java Surface is released by the pending
            // onSurfaceTextureDestroyed handshake (or the next bind).
            surfaceReady = false
            renderChoreographer?.removeFrameCallback(frameCallback)
            renderScheduled = false
            engine.clearRenderSurface()
            return
        }
        if (result == 0) {
            // Engine idle — cancel the optimistically-armed callback and park until
            // the next requestRender() wake (position tick / touch).
            renderChoreographer?.removeFrameCallback(frameCallback)
            renderScheduled = false
        }
        // result > 0: the callback armed above IS the next frame — keep it.
    }

    /** Main thread: accumulate a drag delta without touching the engine lock. */
    private fun addPendingScroll(delta: Float) {
        if (delta == 0f) return
        while (true) {
            val cur = pendingScrollDeltaBits.get()
            val next = (Float.fromBits(cur) + delta).toRawBits()
            if (pendingScrollDeltaBits.compareAndSet(cur, next)) return
        }
    }

    /** Render thread: drain and apply the accumulated drag delta. Also called at
     * the start of the posted end/cancel commands so a trailing delta is applied
     * before the gesture is finalized (never after, which would revive dragging). */
    private fun applyPendingScrollOnRenderThread() {
        val delta = Float.fromBits(pendingScrollDeltaBits.getAndSet(0))
        if (delta != 0f) engine.scrollLyricsBy(delta)
    }

    /**
     * Run a scroll-lifecycle command (begin / end / cancel) on the render thread so
     * ALL engine scroll mutations are ordered on one thread — keeping them after the
     * accumulated deltas and off the main thread's hot path. Falls back to the main
     * thread only if the render thread doesn't exist yet.
     */
    private fun postScrollCommand(command: () -> Unit) {
        val handler = renderHandler
        if (handler != null) handler.post(command) else command()
        requestRender()
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
                    postScrollCommand { engine.beginLyricsScroll() }
                    cancelTapDetection(event)
                    return true
                }

                if (isDragging) {
                    val dy = y - lastTouchY
                    if (dy != 0f) {
                        addPendingScroll(-dy * renderScale)
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
                postScrollCommand {
                    applyPendingScrollOnRenderThread()
                    engine.cancelLyricsScroll()
                }
                recycleTouchState()
                parent?.requestDisallowInterceptTouchEvent(false)
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
        releaseRenderSurface()   // blocking EGL teardown on the render thread
        stopRenderThread()       // quitSafely + join → render thread is fully dead
        engine.close()           // now safe on the main thread: no concurrent access
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

        // Acquire the native window here — this is the only step that needs a
        // JNIEnv, so it must stay on the main thread. Then hand the raw pointer to
        // the render thread, which owns all EGL. setRenderSurfaceFromWindow
        // consumes windowPtr on both success and failure, so we never release it.
        val windowPtr = engine.acquireNativeWindow(surface)
        if (windowPtr == 0L) {
            surface.release()
            renderSurface = null
            return
        }
        renderSurface = surface
        // Build the scene on the main thread before the first frame. This is
        // infrequent (lyrics / style / size change), not a per-frame cost.
        ensureScene(width, height)

        val handler = ensureRenderThread()
        handler.post {
            val ok = engine.setRenderSurfaceFromWindow(windowPtr, frameWidth, frameHeight)
            surfaceReady = ok
            if (ok) scheduleFrame()
            // On failure the window ref is already consumed; the stale Surface held
            // in renderSurface is released by the next releaseRenderSurface().
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

    /**
     * Tear down EGL and release the surface. Blocks the caller (main thread) until
     * the render thread has stopped touching the surface, so it is safe to call
     * right before returning true from onSurfaceTextureDestroyed.
     */
    private fun releaseRenderSurface() {
        val handler = renderHandler
        if (handler != null) {
            val latch = CountDownLatch(1)
            // postAtFrontOfQueue so teardown preempts any queued wake Runnables; on
            // the single-threaded Looper it still runs AFTER any in-flight doFrame.
            handler.postAtFrontOfQueue {
                surfaceReady = false
                renderScheduled = false
                renderChoreographer?.removeFrameCallback(frameCallback)
                engine.clearRenderSurface() // drops the EGL renderer (frees context + window)
                latch.countDown()
            }
            try {
                // The main thread holds NO engine lock here, so teardown acquires it
                // as soon as the current frame releases it (≤ ~1 frame). The timeout
                // is only an ANR safety valve — surfaceReady is already false, so no
                // NEW frame can start regardless.
                if (!latch.await(500, TimeUnit.MILLISECONDS)) {
                    android.util.Log.w("RustSkiaLyricsView", "render surface teardown timed out")
                }
            } catch (_: InterruptedException) {
                Thread.currentThread().interrupt()
            }
        }
        renderSurface?.release()
        renderSurface = null
    }

    /** Create the render thread + handler on demand (recreated after a detach). */
    private fun ensureRenderThread(): Handler {
        renderHandler?.let { return it }
        val thread = HandlerThread("lyrics-render").also { it.start() }
        val handler = Handler(thread.looper)
        renderThread = thread
        renderHandler = handler
        // Choreographer is thread-local: grab the render thread's instance ON it.
        // Posted first, so it is set before any scheduleFrame runs (FIFO queue).
        handler.post { renderChoreographer = Choreographer.getInstance() }
        return handler
    }

    /** Quit + join the render thread. Safe to call when it does not exist. */
    private fun stopRenderThread() {
        val thread = renderThread
        if (thread != null) {
            thread.quitSafely()
            try {
                thread.join(500)
            } catch (_: InterruptedException) {
                Thread.currentThread().interrupt()
            }
        }
        renderThread = null
        renderHandler = null
        renderChoreographer = null
    }

    private fun ensureScene(width: Int, height: Int): Boolean {
        val sceneLyrics = lyrics ?: return false
        updateRenderTarget(width, height)
        val sceneWidth = renderWidth.takeIf { it > 0 } ?: width
        val sceneHeight = renderHeight.takeIf { it > 0 } ?: height
        if (sceneDirty) {
            engine.setLyricsScene(
                sceneLyrics.toSceneJson(
                    sceneWidth,
                    sceneHeight,
                    sceneStyle.scaled(renderScale)
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
        // Mark dirty BEFORE (re)binding so bindRenderSurface's ensureScene rebuilds
        // with the new font. configureFonts may have recreated the engine handle,
        // dropping the GPU renderer, so a rebind is required here.
        sceneDirty = true
        if (isAvailable && width > 0 && height > 0) {
            surfaceTexture?.let { surfaceTexture ->
                bindRenderSurface(surfaceTexture, width, height)
            }
        }
        requestRender()
    }

    private fun resetManualScroll() {
        // Drop any un-applied drag delta so it can't be re-applied after the reset.
        pendingScrollDeltaBits.set(0)
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
        // Apply any trailing drag delta then end the scroll, both on the render
        // thread so the delta lands before endLyricsScroll (a delta applied after
        // it would flip dragging back on and break the fling/return).
        postScrollCommand {
            applyPendingScrollOnRenderThread()
            engine.endLyricsScroll(nativeVelocity)
        }
        recycleTouchState()
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

    private companion object {
        const val MAX_RENDER_PIXELS = 2_200_000f
        const val MIN_RENDER_SCALE = 0.7f
    }
}
