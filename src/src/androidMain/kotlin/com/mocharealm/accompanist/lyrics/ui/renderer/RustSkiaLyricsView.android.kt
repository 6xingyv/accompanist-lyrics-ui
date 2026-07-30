package com.mocharealm.accompanist.lyrics.ui.renderer

import android.animation.ValueAnimator
import android.content.Context
import android.graphics.Color
import android.graphics.Outline
import android.graphics.SurfaceTexture
import android.os.Handler
import android.os.HandlerThread
import android.os.SystemClock
import android.util.AttributeSet
import android.view.Choreographer
import android.view.Surface
import android.view.GestureDetector
import android.view.MotionEvent
import android.view.TextureView
import android.view.VelocityTracker
import android.view.View
import android.view.ViewConfiguration
import android.view.ViewOutlineProvider
import com.mocharealm.accompanist.lyrics.core.model.SyncedLyrics
import com.mocharealm.accompanist.lyrics.text.NativeFontConfig
import com.mocharealm.accompanist.lyrics.text.NativeFontSource
import com.mocharealm.accompanist.lyrics.text.NativeTextEngine
import androidx.compose.ui.unit.Density
import com.mocharealm.accompanist.lyrics.ui.composable.lyrics.KaraokeLyricsConfig
import com.mocharealm.accompanist.lyrics.ui.composable.lyrics.NativePlayerExpansionGeometry
import com.mocharealm.accompanist.lyrics.ui.composable.lyrics.getFontSource
import com.mocharealm.accompanist.lyrics.ui.diagnostics.LyricsUiDiagnostics
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicInteger
import kotlin.math.abs
import kotlin.math.max
import kotlin.math.roundToInt
import kotlin.math.sqrt

private const val PLAYER_EXPANSION_DURATION_MS = 420L

// Playback-clock reconciliation tuning (see `computeDisplayTimeMs`).
/** A gap larger than this between our clock and a fresh sample is a seek → snap. */
private const val CLOCK_SEEK_SNAP_MS = 1000.0
/** Window over which a small gap to the authoritative sample is eased away. */
private const val CLOCK_RECONCILE_MS = 350.0
/** Cap on the catch-up rate when the display clock is behind the sample. */
private const val CLOCK_MAX_RATE = 2.5
/** Clamp on a single frame's dt, so a stall doesn't lurch the clock. */
private const val CLOCK_MAX_FRAME_MS = 64.0
/** Keep the player surface bounded on high-refresh displays. */
private const val TARGET_FRAME_INTERVAL_NANOS = 1_000_000_000L / 60L
private const val FRAME_INTERVAL_TOLERANCE_NANOS = 1_000_000L
/** Some OEMs do not continuously dispatch vsync to a non-UI Looper. */
private const val CHOREOGRAPHER_STALL_TIMEOUT_MS = 50L
/** Recheck the native music-foundation clock without a Kotlin playback-state loop. */
private const val NATIVE_CLOCK_IDLE_POLL_MS = 250L
/** JNI result codes that specifically mean the EGL/window surface is unusable. */
private const val RENDER_SURFACE_MISSING = -20
private const val RENDER_PRESENT_FAILED = -21

class RustSkiaLyricsView @JvmOverloads constructor(
    context: Context,
    attrs: AttributeSet? = null,
    defStyleAttr: Int = 0
) : TextureView(context, attrs, defStyleAttr), TextureView.SurfaceTextureListener {
    internal var retainNativeEngineOnDetach: Boolean = false

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

    init {
        outlineProvider = object : ViewOutlineProvider() {
            override fun getOutline(view: View, outline: Outline) {
                updatePlayerOutline(outline)
            }
        }
        LyricsUiDiagnostics.record(
            "RustSkiaLyricsView",
            "created sdk=${android.os.Build.VERSION.SDK_INT} manufacturer=${android.os.Build.MANUFACTURER} model=${android.os.Build.MODEL}",
        )
    }

    private var lyrics: SyncedLyrics? = null

    // Playback clock. The host publishes only the AUTHORITATIVE position sample
    // (~every 250ms, plus seeks) via `setCurrentPosition`; the render thread
    // extrapolates from that anchor and eases its own smoothed `displayMs` toward it
    // every frame. This replaces the old per-Compose-frame position push: the clock
    // no longer round-trips through Compose, and an authoritative sample that lands
    // BEHIND our extrapolation makes us decelerate until it catches up instead of
    // snapping backward or freezing. `anchor*`/`isPlaying` cross the main→render
    // thread boundary (volatile); `displayMs`/`lastClockNanos`/`clockPrimed` are
    // render-thread-only. `lastRenderedTimeMs` is what we last drew (read by
    // hit-testing on the main thread).
    @Volatile
    private var anchorPositionMs: Int = 0
    @Volatile
    private var anchorClockNanos: Long = 0L
    @Volatile
    private var lastRenderedTimeMs: Int = 0
    private var displayMs: Double = 0.0
    private var lastClockNanos: Long = 0L
    private var clockPrimed = false
    private var sceneDirty = true
    // Coalesce a Compose AndroidView update into at most one scene rebuild.
    private var stateUpdateDepth = 0
    private var sceneRebuildPending = false
    private var renderRequestPending = false
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

    // --- Full-bleed mesh-gradient background state ----------------------------
    // Held here (not just in the engine) because configureFonts recreates the
    // native engine (nativeInit), dropping the renderer's background — so it must
    // be re-applied after every font (re)configuration and rebind.
    private var backgroundPixels: IntArray? = null
    private var backgroundWidth = 0
    private var backgroundHeight = 0
    private var backgroundSeed = 0
    private var backgroundReactive = false
    @Volatile
    private var isPlaying = false
    @Volatile
    private var useMusicFoundationClock = false
    // Content insets (view px). Vertical: `contentTopPx` is the SYSTEM top inset
    // (status + caption bar); the in-surface top bar (below) adds to it.
    // `contentBottomPx` is the navigation-bar inset. Horizontal: `contentLeftPx` /
    // `contentRightPx` are the safe-area sides (display cutout / side nav in
    // landscape) so the top bar and lyrics stay clear of a cutout.
    private var contentTopPx = 0f
    private var contentBottomPx = 0f
    private var contentLeftPx = 0f
    private var contentRightPx = 0f

    // In-surface player top bar (album thumbnail + title/artist + ⋯ button). When a
    // title is set, the engine draws the top bar and the lyrics band starts below it.
    private var topBarTitle: String? = null
    private var topBarArtist: String? = null
    private var onControlsClicked: (() -> Unit)? = null
    private var playerWire: PlayerWire? = null
    private var playerExpansionProgress = 1f
    private var playerExpansionTarget = 1f
    private var playerExpansionGeometry: NativePlayerExpansionGeometry? = null
    private var playerExpansionConfigured = false
    private var playerExpansionAnimator: ValueAnimator? = null
    private var playerExpansionDragStartProgress = 0f
    private var playerExpansionGestureOwnsTarget = false
    private var onPlayerAction: ((Int) -> Unit)? = null
    private data class QueueArtworkPixels(val pixels: IntArray, val width: Int, val height: Int)
    private val queueArtworkPixels = LinkedHashMap<String, QueueArtworkPixels>()

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
    private var lastPresentedFrameNanos = 0L
    private var useHandlerFramePump = false
    private var lastDiagnosticRenderKind = Int.MIN_VALUE
    private var lastDiagnosticFrameLogNanos = 0L
    // Coalesces main-thread wake-ups so a burst of setCurrentPosition / touch
    // events posts at most one Runnable to the render thread.
    private val wakePending = AtomicBoolean(false)
    private val frameCallback: Choreographer.FrameCallback = Choreographer.FrameCallback { frameTimeNanos ->
        renderHandler?.removeCallbacks(choreographerWatchdog)
        doFrame(frameTimeNanos)
    }
    private val handlerFrameRunnable: Runnable = Runnable {
        doFrame(SystemClock.elapsedRealtimeNanos())
    }
    private val choreographerWatchdog: Runnable = Runnable {
        if (!renderScheduled || !surfaceReady || useHandlerFramePump) return@Runnable
        renderChoreographer?.removeFrameCallback(frameCallback)
        renderScheduled = false
        useHandlerFramePump = true
        android.util.Log.w(
            "RustSkiaLyricsView",
            "render-thread Choreographer stalled; switching to Handler frame pump",
        )
        LyricsUiDiagnostics.record(
            "frame-pump",
            "Choreographer stalled after ${CHOREOGRAPHER_STALL_TIMEOUT_MS}ms; switching to Handler",
        )
        doFrame(SystemClock.elapsedRealtimeNanos())
    }
    private val nativeClockIdlePoll: Runnable = Runnable { scheduleFrame() }
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
    private var isPlayerExpansionDragging = false
    private var isQueueReordering = false
    private var downX = 0f
    private var downY = 0f
    private var lastTouchY = 0f
    /** Set once a swallowed expansion-candidate move commits to being a non-tap,
     * so the gesture detector is cancelled exactly once for the gesture. */
    private var swallowedGestureCancelledTap = false
    /** Set when a top-grab-region gesture moves upward past slop: the collapse
     * capture is released and the gesture falls through to the scroll path. */
    private var collapseGrabReleased = false
    private var onLineClicked: ((Int) -> Unit)? = null
    private var onLinePressed: ((Int) -> Unit)? = null
    private var onQueueReordered: ((Int, Int) -> Unit)? = null
    private var onPlayerExpansionDragStart: (() -> Unit)? = null
    private var onPlayerExpansionSettled: ((Float) -> Unit)? = null

    private val gestureDetector = GestureDetector(context, object : GestureDetector.SimpleOnGestureListener() {
        override fun onDown(e: MotionEvent): Boolean = true

        override fun onSingleTapUp(e: MotionEvent): Boolean {
            if (!engineClosed && engine.hitTestTopBar(e.x * renderScale, e.y * renderScale)) {
                performClick()
                onControlsClicked?.invoke()
                return true
            }
            val lineIndex = hitTestLine(e.x, e.y) ?: return false
            performClick()
            onLineClicked?.invoke(lineIndex)
            return true
        }

        override fun onLongPress(e: MotionEvent) {
            if (playerWire != null && !engineClosed) {
                val index = engine.beginQueueReorder(e.x * renderScale, e.y * renderScale)
                if (index >= 0) {
                    isQueueReordering = true
                    velocityTracker?.clear()
                    requestRender()
                    return
                }
            }
            hitTestLine(e.x, e.y)?.let { lineIndex ->
                onLinePressed?.invoke(lineIndex)
            }
        }
    })

    init {
        surfaceTextureListener = this
        // Start non-opaque (mini pill / transitions need alpha); updateSurfaceOpacity
        // flips this to true whenever the renderer is known full-bleed opaque.
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
        if (!engineClosed) {
            val configured = configuredFontBytes
            if (configured === fontBytes) return
            // The prewarmer and the eventual player composition can receive
            // distinct ByteArray instances for the same resource. Compare only on
            // that one identity miss; normal recompositions still take the O(1)
            // branch above and never hash/scan the font repeatedly.
            if (configured != null && fontBytes != null &&
                configured.size == fontBytes.size && configured.contentEquals(fontBytes)
            ) {
                configuredFontBytes = fontBytes
                return
            }
        }

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

    /** Apply one Compose state snapshot atomically, rebuilding the native scene once. */
    internal fun applyStateUpdate(update: RustSkiaLyricsView.() -> Unit) {
        stateUpdateDepth++
        try {
            update()
        } finally {
            stateUpdateDepth--
            if (stateUpdateDepth == 0) {
                val rebuild = sceneRebuildPending
                val render = renderRequestPending
                sceneRebuildPending = false
                renderRequestPending = false
                if (rebuild && width > 0 && height > 0) ensureScene(width, height)
                if (rebuild || render) requestRender()
            }
        }
    }

    fun setLyrics(lyrics: SyncedLyrics?) {
        if (this.lyrics === lyrics) return
        this.lyrics = lyrics
        resetManualScroll()
        sceneDirty = true
        // set_scene_json applies the scene locale to cosmic-text and Android's
        // AFontMatcher while retaining the already-loaded font database. Re-running
        // configureFonts here would replace EngineState and EGL just as the valid
        // empty loading scene is replaced by the new track's lyrics.
        rebuildSceneAndRender()
    }

    /**
     * Publish the authoritative playback position (the value the player reports,
     * projected to "now"). Re-anchors the render-thread clock, which then eases its
     * smoothed display position toward it. Ignores an unchanged sample so a
     * recomposition that re-sends the same value doesn't keep resetting the anchor
     * (which would stall the extrapolation).
     */
    fun setCurrentPosition(currentTimeMs: Int) {
        if (currentTimeMs == anchorPositionMs) return
        anchorPositionMs = currentTimeMs
        anchorClockNanos = SystemClock.elapsedRealtimeNanos()
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
     * Install album artwork for the GPU mesh-gradient background (enabling the
     * full-bleed background). Pass `null` to disable it. Deduped by a content hash
     * so repeated calls with the same artwork don't rebuild the mesh.
     */
    fun setBackgroundArt(pixels: IntArray?, width: Int, height: Int) {
        if (pixels == null || width <= 0 || height <= 0) {
            if (backgroundPixels != null) {
                backgroundPixels = null
                updateSurfaceOpacity()
                if (!engineClosed) engine.clearBackground()
                requestRender()
            }
            return
        }
        // The Compose host remembers this IntArray. Check identity before the
        // O(pixel-count) hash so ordinary recompositions stay O(1).
        if (backgroundPixels === pixels && width == backgroundWidth && height == backgroundHeight) {
            return
        }
        val seed = pixels.contentHashCode()
        if (backgroundPixels != null && seed == backgroundSeed &&
            width == backgroundWidth && height == backgroundHeight
        ) {
            return
        }
        backgroundPixels = pixels
        backgroundWidth = width
        backgroundHeight = height
        backgroundSeed = seed
        updateSurfaceOpacity()
        applyBackgroundArt()
        requestRender()
    }

    /**
     * Vertical content insets (view px) for the lyrics band: `topPx` = status bar +
     * top bar, `bottomPx` = navigation bar. Lyrics are clipped and edge-faded to the
     * band; the background still fills the whole surface.
     */
    fun setContentInsets(topPx: Float, bottomPx: Float, leftPx: Float, rightPx: Float) {
        if (contentTopPx == topPx && contentBottomPx == bottomPx &&
            contentLeftPx == leftPx && contentRightPx == rightPx
        ) {
            return
        }
        contentTopPx = topPx
        contentBottomPx = bottomPx
        contentLeftPx = leftPx
        contentRightPx = rightPx
        sceneDirty = true
        rebuildSceneAndRender()
    }

    /** Set the in-surface top bar's title/artist (null title disables it). */
    fun setTopBar(title: String?, artist: String?) {
        if (topBarTitle == title && topBarArtist == artist) return
        topBarTitle = title
        topBarArtist = artist
        sceneDirty = true
        rebuildSceneAndRender()
    }

    /** Callback for a tap on the top bar's ⋯ button. */
    fun setOnControlsClicked(callback: (() -> Unit)?) {
        onControlsClicked = callback
    }

    /** Enable the complete Rust-rendered portrait player. Passing a null title
     * returns to the legacy lyrics-only surface. */
    fun setPlayerChrome(
        title: String?,
        artist: String = "",
        durationMs: Int = 0,
        playing: Boolean = false,
        liked: Boolean = false,
        presentation: String = "full",
        viewportWidth: Float? = null,
        viewportHeight: Float? = null,
        miniForegroundArgb: Int = -1,
        screen: String = "artwork",
        queueTitle: String = "",
        queueSource: String = "",
        queueFilter: String = "upNext",
        queueItems: List<Triple<String, String, String?>> = emptyList(),
    ) {
        require(screen == "lyrics" || screen == "artwork" || screen == "queue") {
            "screen must be lyrics, artwork, or queue"
        }
        require(presentation == "mini" || presentation == "full") {
            "presentation must be mini or full"
        }
        require(queueFilter in setOf("upNext", "shuffle", "repeatOne", "album")) {
            "queueFilter must be upNext, shuffle, repeatOne, or album"
        }
        val next = title?.let {
            PlayerWire(
                presentation = presentation,
                viewportWidth = viewportWidth,
                viewportHeight = viewportHeight,
                miniForegroundArgb = miniForegroundArgb,
                screen = screen,
                title = it,
                artist = artist,
                durationMs = durationMs,
                isPlaying = playing,
                liked = liked,
                queueTitle = queueTitle,
                queueSource = queueSource,
                queueFilter = queueFilter,
                queueItems = queueItems.map { (itemTitle, itemArtist, artworkKey) ->
                    PlayerQueueItemWire(
                        title = itemTitle,
                        artist = itemArtist,
                        artworkKey = artworkKey.orEmpty(),
                    )
                },
            )
        }
        if (playerWire == next) return
        playerWire = next
        // presentation mini <-> full flips whether the renderer clears transparent.
        updateSurfaceOpacity()
        sceneDirty = true
        rebuildSceneAndRender()
    }

    fun setPlayerExpansionProgress(progress: Float) {
        val next = progress.coerceIn(0f, 1f)
        cancelPlayerExpansionAnimator()
        playerExpansionTarget = next
        if (playerExpansionProgress == next) {
            // No visual change, but the animator was just cancelled (and the
            // geometry-null host has no applyPlayerExpansionVisual pass).
            updateSurfaceOpacity()
            return
        }
        playerExpansionProgress = next
        applyPlayerExpansionVisual(next)
        updateSurfaceOpacity()
        if (!engineClosed) postPlayerCommand { engine.setPlayerExpansionProgress(next) }
    }

    fun configurePlayerExpansion(
        geometry: NativePlayerExpansionGeometry?,
        target: Float,
    ) {
        val nextTarget = target.coerceIn(0f, 1f)
        val firstConfiguration = !playerExpansionConfigured
        val geometryChanged = playerExpansionGeometry != geometry
        playerExpansionGeometry = geometry
        playerExpansionConfigured = geometry != null
        if (geometry == null) {
            cancelPlayerExpansionAnimator()
            playerExpansionProgress = nextTarget
            playerExpansionTarget = nextTarget
            translationX = 0f
            translationY = 0f
            clipToOutline = false
            updateSurfaceOpacity()
            return
        }
        if (firstConfiguration) {
            playerExpansionProgress = nextTarget
            playerExpansionTarget = nextTarget
            applyPlayerExpansionVisual(nextTarget)
            if (!engineClosed) postPlayerCommand {
                engine.setPlayerExpansionProgress(nextTarget)
            }
            return
        }
        if (geometryChanged) applyPlayerExpansionVisual(playerExpansionProgress)
        if (isPlayerExpansionDragging || playerExpansionGestureOwnsTarget) return
        if (nextTarget != playerExpansionTarget) animatePlayerExpansionTo(nextTarget)
    }

    private fun animatePlayerExpansionTo(target: Float) {
        val next = target.coerceIn(0f, 1f)
        cancelPlayerExpansionAnimator()
        playerExpansionTarget = next
        if (next == playerExpansionProgress) {
            applyPlayerExpansionVisual(next)
            playerExpansionGestureOwnsTarget = false
            onPlayerExpansionSettled?.invoke(next)
            return
        }
        if (!engineClosed) postPlayerCommand {
            engine.animatePlayerExpansionTo(next, PLAYER_EXPANSION_DURATION_MS.toFloat())
        }
        playerExpansionAnimator = ValueAnimator.ofFloat(playerExpansionProgress, next).apply {
            duration = PLAYER_EXPANSION_DURATION_MS
            interpolator = android.animation.TimeInterpolator { input ->
                input * input * (3f - 2f * input)
            }
            addUpdateListener { animator ->
                playerExpansionProgress = animator.animatedValue as Float
                applyPlayerExpansionVisual(playerExpansionProgress)
            }
            addListener(object : android.animation.AnimatorListenerAdapter() {
                override fun onAnimationEnd(animation: android.animation.Animator) {
                    if (playerExpansionAnimator !== animation) return
                    playerExpansionAnimator = null
                    playerExpansionProgress = next
                    applyPlayerExpansionVisual(next)
                    playerExpansionGestureOwnsTarget = false
                    onPlayerExpansionSettled?.invoke(next)
                }
            })
            start()
        }
        // The animator is now live: drop opacity BEFORE its first tick so the
        // renderer's transparent transition clear never lands on an opaque layer.
        updateSurfaceOpacity()
    }

    private fun cancelPlayerExpansionAnimator() {
        val animator = playerExpansionAnimator
        playerExpansionAnimator = null
        animator?.cancel()
    }

    private fun applyInteractivePlayerExpansion(progress: Float) {
        val next = progress.coerceIn(0f, 1f)
        playerExpansionProgress = next
        playerExpansionTarget = next
        applyPlayerExpansionVisual(next)
        if (!engineClosed) postPlayerCommand { engine.setPlayerExpansionProgress(next) }
    }

    private fun applyPlayerExpansionVisual(progress: Float) {
        val geometry = playerExpansionGeometry ?: return
        val p = progress.coerceIn(0f, 1f)
        translationX = geometry.collapsedLeft * (1f - p)
        translationY = geometry.collapsedTop * (1f - p)
        // The mini background is owned by Compose. A native elevation shadow is
        // clipped by the collapsed host layer and darkens the inside of the pill.
        elevation = 0f
        clipToOutline = p < 1f || playerExpansionAnimator != null || isPlayerExpansionDragging
        invalidateOutline()
        updateSurfaceOpacity()
    }

    /**
     * Keep [isOpaque] in sync with whether the renderer actually fills every pixel.
     * A full-screen non-opaque TextureView forces HWUI to alpha-blend the whole
     * texture every frame, so flip opaque whenever the surface is known full-bleed:
     * the Rust renderer clears the canvas to opaque BLACK + mesh only when the
     * mesh-gradient background is enabled AND the player is not in mini
     * presentation AND the (engine-side) expansion is >= 0.999 — in every other
     * case (mini pill, expansion transition, no background art) it clears
     * transparent, so the view must stay non-opaque. The Kotlin-side steady-state
     * condition (progress == 1, no animator, no drag) is the inverse of the
     * `clipToOutline` condition above (clip on ⇒ rounded corners ⇒ non-opaque).
     * Toggling isOpaque on a live TextureView takes effect on the next
     * layer/buffer update, so we invalidate() to apply it promptly.
     */
    private fun updateSurfaceOpacity() {
        val fullBleed = backgroundPixels != null &&
            playerWire?.presentation != "mini" &&
            playerExpansionProgress >= 1f &&
            playerExpansionAnimator == null &&
            !isPlayerExpansionDragging
        if (isOpaque != fullBleed) {
            isOpaque = fullBleed
            invalidate()
        }
    }

    private fun updatePlayerOutline(outline: Outline) {
        val geometry = playerExpansionGeometry ?: run {
            outline.setRect(0, 0, width, height)
            return
        }
        val p = playerExpansionProgress.coerceIn(0f, 1f)
        val clipWidth = geometry.collapsedWidth + (width - geometry.collapsedWidth) * p
        val clipHeight = geometry.collapsedHeight + (height - geometry.collapsedHeight) * p
        // TextureView + arbitrary Path outlines are expensive and have stalled
        // some vendor renderers. Screen corners are symmetric on supported
        // Android devices, so use the hardware round-rect outline fast path.
        val expandedRadius = maxOf(
            geometry.expandedTopLeftRadius,
            geometry.expandedTopRightRadius,
            geometry.expandedBottomRightRadius,
            geometry.expandedBottomLeftRadius,
        )
        val radius = geometry.collapsedRadius +
            (expandedRadius - geometry.collapsedRadius) * p
        outline.setRoundRect(
            0,
            0,
            clipWidth.roundToInt().coerceAtLeast(1),
            clipHeight.roundToInt().coerceAtLeast(1),
            radius.coerceAtLeast(0f),
        )
    }

    /** Stable native action codes: favorite=1, more=2, previous=3,
     * play/pause=4, next=5, lyrics=6, output=7, queue=8; queue filters=9..12. */
    fun setOnPlayerAction(callback: ((Int) -> Unit)?) {
        onPlayerAction = callback
    }

    fun setOnPlayerExpansionDragCallbacks(
        onStart: (() -> Unit)?,
        onSettled: ((Float) -> Unit)?,
    ) {
        onPlayerExpansionDragStart = onStart
        onPlayerExpansionSettled = onSettled
    }

    fun setOnQueueReordered(callback: ((Int, Int) -> Unit)?) {
        onQueueReordered = callback
    }

    fun clearQueueArtworks() {
        queueArtworkPixels.clear()
        if (!engineClosed) postPlayerCommand { engine.clearQueueArtworks() }
    }

    fun setQueueArtwork(key: String, pixels: IntArray, width: Int, height: Int) {
        val current = queueArtworkPixels[key]
        if (current?.pixels === pixels && current.width == width && current.height == height) return
        queueArtworkPixels[key] = QueueArtworkPixels(pixels, width, height)
        if (!engineClosed) postPlayerCommand {
            engine.setQueueArtwork(key, pixels, width, height)
        }
    }

    /**
     * Resolve the top-bar geometry (in render px) + the lyrics band's top inset. The
     * layout mirrors the old Compose top bar (28dp padding, 68dp thumbnail, 8dp gap,
     * 17sp/15sp text, a ⋯ button on the right). Returns `(wire, contentTopPx)` where
     * `contentTopPx` is the system inset alone when no top bar is set.
     */
    private fun resolveTopBar(sceneWidth: Int): Pair<TopBarWire?, Float> {
        val s = renderScale
        val sysTop = contentTopPx * s
        // Horizontal safe-area insets (render px): keep the top bar clear of a
        // landscape display cutout / side nav bar on either edge.
        val sysLeft = contentLeftPx * s
        val sysRight = contentRightPx * s
        val title = topBarTitle ?: return null to sysTop

        val d = resources.displayMetrics.density
        val fs = resources.configuration.fontScale
        fun dp(v: Float) = v * d * s
        fun sp(v: Float) = v * d * fs * s

        val barTop = sysTop + dp(28f)
        val thumbSize = dp(68f)
        val thumbLeft = sysLeft + dp(28f)
        val gap = dp(8f)
        val textLeft = thumbLeft + thumbSize + gap
        val titleFontSize = sp(17f)
        val artistFontSize = sp(15f)
        val titleLineHeight = titleFontSize * 1.3f
        val artistLineHeight = artistFontSize * 1.3f
        val textBlockHeight = titleLineHeight + artistLineHeight
        val titleTop = barTop + (thumbSize - textBlockHeight) / 2f
        val artistTop = titleTop + titleLineHeight
        val buttonRadius = dp(14f)
        val buttonCx = sceneWidth - sysRight - dp(28f) - buttonRadius
        val buttonCy = barTop + thumbSize / 2f
        val textMaxWidth = (buttonCx - buttonRadius - gap - textLeft).coerceAtLeast(1f)

        val wire = TopBarWire(
            title = title,
            artist = topBarArtist ?: "",
            thumbLeft = thumbLeft,
            thumbTop = barTop,
            thumbSize = thumbSize,
            thumbRadius = dp(14f),
            textLeft = textLeft,
            textMaxWidth = textMaxWidth,
            titleTop = titleTop,
            titleFontSize = titleFontSize,
            titleLineHeight = titleLineHeight,
            titleWeight = 600,
            artistTop = artistTop,
            artistFontSize = artistFontSize,
            artistLineHeight = artistLineHeight,
            artistAlpha = 0.4f,
            buttonCx = buttonCx,
            buttonCy = buttonCy,
            buttonRadius = buttonRadius,
        )
        return wire to (barTop + thumbSize + dp(20f))
    }

    /** Drive the background: `playing` gates the time flow, `reactive` the audio reactivity. */
    fun setPlaybackState(playing: Boolean, reactive: Boolean) {
        if (isPlaying == playing && backgroundReactive == reactive) {
            // Under the native music-foundation clock the render loop parks fully
            // while (host-paused AND engine-idle). The native clock can start
            // slightly before the Kotlin `playing` flag lands, so ANY playback push
            // still kicks exactly one frame, letting the engine re-report activity
            // and un-park; if nothing changed the frame goes idle and re-parks.
            if (useMusicFoundationClock) requestRender()
            return
        }
        LyricsUiDiagnostics.record(
            "playback-input",
            "playing=$playing reactive=$reactive nativeClock=$useMusicFoundationClock",
        )
        if (isPlaying != playing) {
            // Fold the elapsed play time into the anchor before flipping play/pause,
            // so the clock resumes/freezes at the current position instead of a stale
            // anchor.
            val now = SystemClock.elapsedRealtimeNanos()
            if (isPlaying) {
                anchorPositionMs += ((now - anchorClockNanos) / 1_000_000L).toInt()
            }
            anchorClockNanos = now
        }
        isPlaying = playing
        backgroundReactive = reactive
        if (!engineClosed) engine.setPlaybackState(playing, reactive)
        requestRender()
    }

    /**
     * Let the render JNI read music-foundation's lock-free native clock directly.
     * This is optional so lyrics-ui keeps working in hosts that do not bundle the
     * playback engine.
     */
    fun setMusicFoundationClockEnabled(enabled: Boolean) {
        if (useMusicFoundationClock == enabled) return
        useMusicFoundationClock = enabled
        if (!enabled && !engineClosed) {
            // The last native-clock frame may have stored local pause/duration
            // overrides in Rust. A remote route is host-clocked, so retaining
            // those values would override its playing state and progress.
            engine.clearPlayerLivePlayback()
            // The prepared player itself already contains the last override.
            // Rebuild from playerWire even when its host value stayed `true`
            // across the route switch and would otherwise be deduplicated.
            sceneDirty = true
            rebuildSceneAndRender()
        }
        LyricsUiDiagnostics.record("clock", "musicFoundationClockEnabled=$enabled")
        requestRender()
    }

    /**
     * Render-thread clock tick: extrapolate the authoritative anchor to now, then ease
     * the smoothed [displayMs] toward it. A far jump (a real seek) snaps; otherwise the
     * rate is `base + gap/RECONCILE_MS` clamped to `[0, MAX_RATE]` — so when the anchor
     * lands BEHIND us (gap < 0) our rate drops below realtime (down to a pause, never
     * reversing) until it catches up, and when it lands ahead we speed up to close the
     * gap smoothly rather than jumping.
     */
    private fun computeDisplayTimeMs(): Int {
        val now = SystemClock.elapsedRealtimeNanos()
        val playing = isPlaying
        val elapsedMs = if (playing) (now - anchorClockNanos) / 1_000_000.0 else 0.0
        val target = anchorPositionMs + elapsedMs
        if (!clockPrimed) {
            clockPrimed = true
            lastClockNanos = now
            displayMs = target
            return displayMs.roundToInt()
        }
        val dtMs = ((now - lastClockNanos) / 1_000_000.0).coerceIn(0.0, CLOCK_MAX_FRAME_MS)
        lastClockNanos = now
        val gap = target - displayMs
        if (abs(gap) >= CLOCK_SEEK_SNAP_MS) {
            displayMs = target
        } else {
            val baseRate = if (playing) 1.0 else 0.0
            val rate = (baseRate + gap / CLOCK_RECONCILE_MS).coerceIn(0.0, CLOCK_MAX_RATE)
            displayMs += rate * dtMs
            // Catching up from behind: don't overshoot past the target.
            if (gap > 0.0 && displayMs > target) displayMs = target
        }
        return displayMs.roundToInt()
    }

    /** (Re)apply the held background art + playback state to the native engine. */
    private fun applyBackgroundArt() {
        val pixels = backgroundPixels ?: return
        if (engineClosed) return
        engine.setBackgroundArt(pixels, backgroundWidth, backgroundHeight, backgroundSeed)
        engine.setPlaybackState(isPlaying, backgroundReactive)
    }

    /**
     * Rebuild the (dirty) scene on the main thread — infrequent, so keeping it here
     * avoids threading the lyrics/style snapshot onto the render thread — then wake
     * the render thread to draw it.
     */
    private fun rebuildSceneAndRender() {
        if (stateUpdateDepth > 0) {
            sceneRebuildPending = true
            renderRequestPending = true
            return
        }
        if (width > 0 && height > 0) ensureScene(width, height)
        requestRender()
    }

    override fun onSurfaceTextureAvailable(surface: SurfaceTexture, width: Int, height: Int) {
        LyricsUiDiagnostics.record("surface", "available ${width}x$height attached=$isAttachedToWindow")
        updateRenderTarget(width, height)
        sceneDirty = true
        bindRenderSurface(surface, width, height)
    }

    override fun onSurfaceTextureSizeChanged(surface: SurfaceTexture, width: Int, height: Int) {
        LyricsUiDiagnostics.record("surface", "sizeChanged ${width}x$height")
        updateRenderTarget(width, height)
        sceneDirty = true
        bindRenderSurface(surface, width, height)
    }

    override fun onSurfaceTextureDestroyed(surface: SurfaceTexture): Boolean {
        LyricsUiDiagnostics.record("surface", "destroyed")
        // Blocks until the render thread has torn down EGL and stopped touching the
        // surface, so returning true (which frees the SurfaceTexture) is safe.
        releaseRenderSurface()
        return true
    }

    override fun onSurfaceTextureUpdated(surface: SurfaceTexture) {
    }

    override fun onWindowVisibilityChanged(visibility: Int) {
        super.onWindowVisibilityChanged(visibility)
        if (visibility != View.VISIBLE) {
            // TextureView may retain its SurfaceTexture while the app is in the
            // background, so no destroyed/available callback is guaranteed. Drop
            // EGL explicitly and rebuild it on return to refresh buffer geometry.
            releaseRenderSurface()
            return
        }

        if (renderSurface == null && isAvailable && width > 0 && height > 0 && !engineClosed) {
            surfaceTexture?.let { bindRenderSurface(it, width, height) }
        }
    }

    /**
     * Wake the render loop. Runs on the main thread and only hops one coalesced
     * Runnable over to the render thread, where [scheduleFrame] posts a
     * vsync-paced frame. No-op until the render thread exists (the surface
     * callbacks create it).
     */
    private fun requestRender() {
        if (stateUpdateDepth > 0) {
            renderRequestPending = true
            return
        }
        val handler = renderHandler ?: return
        if (wakePending.compareAndSet(false, true)) {
            handler.post(wakeRunnable)
        }
    }

    /** Render thread only: schedule one frame on the next vsync (deduped). */
    private fun scheduleFrame() {
        if (renderScheduled || !surfaceReady) return
        val handler = renderHandler ?: return
        handler.removeCallbacks(nativeClockIdlePoll)
        if (useHandlerFramePump) {
            renderScheduled = true
            val now = SystemClock.elapsedRealtimeNanos()
            val dueNanos = if (lastPresentedFrameNanos == 0L) {
                now
            } else {
                lastPresentedFrameNanos + TARGET_FRAME_INTERVAL_NANOS
            }
            val delayMs = ((dueNanos - now).coerceAtLeast(0L) + 999_999L) / 1_000_000L
            handler.postDelayed(handlerFrameRunnable, delayMs)
            return
        }

        // Choreographer is installed asynchronously on the render Looper. If it is
        // unavailable, immediately use the same Handler fallback as an OEM stall.
        val choreographer = renderChoreographer
        if (choreographer == null) {
            useHandlerFramePump = true
            scheduleFrame()
            return
        }
        renderScheduled = true
        choreographer.postFrameCallback(frameCallback)
        handler.postDelayed(choreographerWatchdog, CHOREOGRAPHER_STALL_TIMEOUT_MS)
    }

    /** Render thread only: cancel either frame-pump implementation. */
    private fun cancelScheduledFrame() {
        renderChoreographer?.removeFrameCallback(frameCallback)
        renderHandler?.removeCallbacks(choreographerWatchdog)
        renderHandler?.removeCallbacks(handlerFrameRunnable)
        renderHandler?.removeCallbacks(nativeClockIdlePoll)
        renderScheduled = false
    }

    /**
     * Render thread only: draw + present one frame and keep the loop alive while
     * the engine reports animation/scroll activity (return > 0). When it returns
     * 0 the loop parks until the main thread calls [requestRender] again.
     */
    private fun doFrame(frameTimeNanos: Long) {
        renderHandler?.removeCallbacks(choreographerWatchdog)
        renderScheduled = false
        if (!surfaceReady) return
        val sinceLastPresent = frameTimeNanos - lastPresentedFrameNanos
        if (
            lastPresentedFrameNanos != 0L &&
            sinceLastPresent < TARGET_FRAME_INTERVAL_NANOS - FRAME_INTERVAL_TOLERANCE_NANOS
        ) {
            scheduleFrame()
            return
        }
        lastPresentedFrameNanos = frameTimeNanos
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
        val frameTimeMs = computeDisplayTimeMs()
        lastRenderedTimeMs = frameTimeMs
        val result = if (useMusicFoundationClock) {
            engine.renderLyricsFrameToSurfaceFromMusicFoundation(frameTimeMs)
        } else {
            engine.renderLyricsFrameToSurface(frameTimeMs)
        }
        if (result == RENDER_SURFACE_MISSING || result == RENDER_PRESENT_FAILED) {
            LyricsUiDiagnostics.record(
                "render",
                "surface lost result=$result timeMs=$frameTimeMs handlerPump=$useHandlerFramePump",
            )
            // Surface lost — drop EGL. The Java Surface is released by the pending
            // onSurfaceTextureDestroyed handshake (or the next bind).
            surfaceReady = false
            cancelScheduledFrame()
            engine.clearRenderSurface()
            return
        }
        if (result < 0) {
            // Renderer/scene errors (for example the JNI panic guard's -22) do
            // not invalidate EGL. Clearing the render surface here used to turn
            // one bad transition frame into a permanent black screen because a
            // still-alive TextureView does not emit another available callback.
            LyricsUiDiagnostics.record(
                "render",
                "frame rejected result=$result timeMs=$frameTimeMs; retaining surface",
            )
            parkOrPollIdle()
            return
        }
        val renderKind = result.coerceIn(0, 1)
        if (renderKind != lastDiagnosticRenderKind) {
            lastDiagnosticRenderKind = renderKind
            LyricsUiDiagnostics.record(
                "render",
                "activityChanged result=$result timeMs=$frameTimeMs nativeClock=$useMusicFoundationClock handlerPump=$useHandlerFramePump",
            )
        }
        if (frameTimeNanos - lastDiagnosticFrameLogNanos >= 5_000_000_000L) {
            lastDiagnosticFrameLogNanos = frameTimeNanos
            LyricsUiDiagnostics.record(
                "render",
                "heartbeat result=$result timeMs=$frameTimeMs surfaceReady=$surfaceReady scheduled=$renderScheduled nativeClock=$useMusicFoundationClock handlerPump=$useHandlerFramePump",
            )
        }
        if (result == 0) {
            // Engine idle — cancel the optimistically-armed callback and park or
            // slow-poll depending on the host-pushed playback state.
            parkOrPollIdle()
        }
        // result > 0: the callback armed above IS the next frame — keep it.
    }

    /**
     * Render thread only: the engine reported idle (or rejected a frame without
     * losing the surface). A standalone renderer always parks until
     * [requestRender]. Under the native music-foundation clock we slow-poll
     * (250 ms) ONLY while the host-pushed state says we are playing — the native
     * clock can start slightly before the Kotlin `isPlaying` push lands, so the
     * poll bridges that gap without a Kotlin round trip. When the host says
     * paused too, park COMPLETELY instead of burning ~4 render+present per
     * second forever: every push that could change output already funnels
     * through [requestRender] (setPlaybackState on play/pause, setCurrentPosition
     * on seeks, scene/style/chrome rebuilds, touch/scroll/expansion commands,
     * and the visibility rebind's post-bind scheduleFrame), and each such kick
     * runs one full frame in which the engine can re-report activity and restart
     * the 60 FPS loop.
     */
    private fun parkOrPollIdle() {
        cancelScheduledFrame()
        if (useMusicFoundationClock && surfaceReady && isPlaying) {
            renderHandler?.postDelayed(nativeClockIdlePoll, NATIVE_CLOCK_IDLE_POLL_MS)
        }
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
                if (playerWire != null && !isInsideVisiblePlayer(event.x, event.y)) return false
                parent?.requestDisallowInterceptTouchEvent(true)
                isDragging = false
                isPlayerExpansionDragging = false
                isQueueReordering = false
                swallowedGestureCancelledTap = false
                collapseGrabReleased = false
                activePointerId = event.getPointerId(0)
                downX = event.x
                downY = event.y
                lastTouchY = event.y
                velocityTracker?.recycle()
                velocityTracker = VelocityTracker.obtain().also { it.addMovement(event) }
                if (playerWire != null) {
                    val x = event.x * renderScale
                    val y = event.y * renderScale
                    postPlayerCommand { engine.playerPointerDown(x, y) }
                }
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
                if (isQueueReordering) {
                    val renderY = y * renderScale
                    postPlayerCommand { engine.updateQueueReorder(renderY) }
                    return true
                }
                if (isPlayerExpansionDragging) {
                    val totalDy = y - downY
                    val next = playerExpansionDragStartProgress -
                        totalDy / height.coerceAtLeast(1)
                    applyInteractivePlayerExpansion(next)
                    lastTouchY = y
                    return true
                }
                val miniCanExpand = playerWire?.presentation == "mini" &&
                    playerExpansionProgress < 0.999f &&
                    onPlayerExpansionDragStart != null
                val fullCanCollapse = playerWire?.presentation == "full" &&
                    playerExpansionProgress >= 0.999f &&
                    !collapseGrabReleased &&
                    downY <= 80f * resources.displayMetrics.density
                if (miniCanExpand || fullCanCollapse) {
                    val totalDy = y - downY
                    val totalDx = event.getX(pointerIndex) - downX
                    val startsExpansion = miniCanExpand && totalDy < -touchSlop
                    val startsCollapse = fullCanCollapse && totalDy > touchSlop
                    if (startsExpansion || startsCollapse) {
                        isPlayerExpansionDragging = true
                        playerExpansionGestureOwnsTarget = true
                        cancelPlayerExpansionAnimator()
                        playerExpansionDragStartProgress = playerExpansionProgress
                        lastTouchY = y
                        cancelTapDetection(event)
                        postPlayerCommand { engine.cancelPlayerPointer() }
                        onPlayerExpansionDragStart?.invoke()
                        val next = playerExpansionDragStartProgress -
                            totalDy / height.coerceAtLeast(1)
                        applyInteractivePlayerExpansion(next)
                        return true
                    }
                    if (fullCanCollapse && totalDy < -touchSlop) {
                        // An upward move from the top grab region is a lyric
                        // scroll, not a collapse: release the capture and fall
                        // through to the normal scroll path below for the rest
                        // of the gesture.
                        collapseGrabReleased = true
                    } else {
                        // The mini player and the fullscreen top grab region own
                        // the gesture while deciding between a tap and expansion
                        // drag. Once a swallowed move commits to a non-tap
                        // (past slop in a direction we don't turn into a drag),
                        // kill tap detection too — otherwise a sideways swipe
                        // would end with onSingleTapUp firing at the release
                        // point.
                        if (!swallowedGestureCancelledTap &&
                            (abs(totalDx) > touchSlop || abs(totalDy) > touchSlop)
                        ) {
                            swallowedGestureCancelledTap = true
                            cancelTapDetection(event)
                        }
                        return true
                    }
                }
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
                if (isPlayerExpansionDragging) {
                    velocityTracker?.addMovement(event)
                    velocityTracker?.computeCurrentVelocity(1000, maxFlingVelocity.toFloat())
                    val velocityY = velocityTracker?.getYVelocity(activePointerId) ?: 0f
                    val target = when {
                        velocityY < -700f -> 1f
                        velocityY > 700f -> 0f
                        playerExpansionProgress >= 0.5f -> 1f
                        else -> 0f
                    }
                    animatePlayerExpansionTo(target)
                    postPlayerCommand { engine.cancelPlayerPointer() }
                    recycleTouchState()
                    parent?.requestDisallowInterceptTouchEvent(false)
                    return true
                }
                if (isQueueReordering) {
                    isQueueReordering = false
                    postPlayerCommand {
                        val packed = engine.finishQueueReorder()
                        if (packed >= 0L) {
                            val from = (packed ushr 32).toInt()
                            val to = packed.toInt()
                            if (from != to) post { onQueueReordered?.invoke(from, to) }
                        }
                    }
                    recycleTouchState()
                    parent?.requestDisallowInterceptTouchEvent(false)
                    return true
                }
                val wasDragging = isDragging
                velocityTracker?.addMovement(event)
                if (wasDragging) {
                    finishManualDrag()
                } else {
                    if (playerWire != null) {
                        val x = event.x * renderScale
                        val y = event.y * renderScale
                        postPlayerCommand {
                            val action = engine.playerPointerUp(x, y)
                            if (action != 0) post {
                                performClick()
                                onPlayerAction?.invoke(action)
                            }
                        }
                    }
                    recycleTouchState()
                    parent?.requestDisallowInterceptTouchEvent(false)
                    return gestureDetector.onTouchEvent(event) || super.onTouchEvent(event)
                }
                parent?.requestDisallowInterceptTouchEvent(false)
                return true
            }

            MotionEvent.ACTION_CANCEL -> {
                if (isPlayerExpansionDragging) {
                    animatePlayerExpansionTo(
                        if (playerExpansionProgress >= 0.5f) 1f else 0f
                    )
                }
                if (isQueueReordering) {
                    isQueueReordering = false
                    postPlayerCommand { engine.cancelQueueReorder() }
                }
                if (playerWire != null) postPlayerCommand { engine.cancelPlayerPointer() }
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

    private fun isInsideVisiblePlayer(x: Float, y: Float): Boolean {
        val player = playerWire ?: return true
        val collapsedWidth = player.viewportWidth ?: width.toFloat()
        val collapsedHeight = player.viewportHeight ?: height.toFloat()
        val progress = playerExpansionProgress.coerceIn(0f, 1f)
        val visibleWidth = collapsedWidth + (width - collapsedWidth) * progress
        val visibleHeight = collapsedHeight + (height - collapsedHeight) * progress
        return x >= 0f && y >= 0f && x <= visibleWidth && y <= visibleHeight
    }

    override fun performClick(): Boolean {
        super.performClick()
        return true
    }

    override fun onDetachedFromWindow() {
        cancelPlayerExpansionAnimator()
        super.onDetachedFromWindow()
        releaseRenderSurface()   // blocking EGL teardown on the render thread
        stopRenderThread()       // quitSafely + join → render thread is fully dead
        if (!retainNativeEngineOnDetach) closeNativeEngine()
    }

    private fun postPlayerCommand(command: () -> Unit) {
        val handler = renderHandler
        if (handler != null) handler.post(command) else command()
        requestRender()
    }

    internal fun disposeNativeEngineWhenDetached() {
        retainNativeEngineOnDetach = false
        if (!isAttachedToWindow) closeNativeEngine()
    }

    private fun closeNativeEngine() {
        if (engineClosed) return
        engine.close()           // safe after the render thread is fully stopped
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
        LyricsUiDiagnostics.record(
            "surface",
            "binding view=${width}x$height frame=${frameWidth}x$frameHeight scale=$renderScale",
        )
        surfaceTexture.setDefaultBufferSize(frameWidth, frameHeight)

        val surface = Surface(surfaceTexture)
        requestPlayerFrameRate(surface)

        // Acquire the native window here — this is the only step that needs a
        // JNIEnv, so it must stay on the main thread. Then hand the raw pointer to
        // the render thread, which owns all EGL. setRenderSurfaceFromWindow
        // consumes windowPtr on both success and failure, so we never release it.
        val windowPtr = engine.acquireNativeWindow(surface)
        if (windowPtr == 0L) {
            LyricsUiDiagnostics.record("surface", "acquireNativeWindow failed")
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
            lastPresentedFrameNanos = 0L
            useHandlerFramePump = false
            val ok = engine.setRenderSurfaceFromWindow(windowPtr, frameWidth, frameHeight)
            surfaceReady = ok
            LyricsUiDiagnostics.record(
                "surface",
                "native surface bind ok=$ok frame=${frameWidth}x$frameHeight",
            )
            if (ok) scheduleFrame()
            // On failure the window ref is already consumed; the stale Surface held
            // in renderSurface is released by the next releaseRenderSurface().
        }
    }

    /**
     * Hint that this surface is intentionally capped at 60 FPS. The render loop
     * also enforces the cap because the platform may choose a different mode.
     */
    private fun requestPlayerFrameRate(surface: Surface) {
        if (android.os.Build.VERSION.SDK_INT < android.os.Build.VERSION_CODES.R) return
        runCatching {
            surface.setFrameRate(60f, Surface.FRAME_RATE_COMPATIBILITY_DEFAULT)
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
                lastPresentedFrameNanos = 0L
                useHandlerFramePump = false
                cancelScheduledFrame()
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
            val (topBarWire, resolvedContentTop) = if (playerWire == null) {
                resolveTopBar(sceneWidth)
            } else {
                null to 0f
            }
            val sceneApplied = engine.setLyricsSceneDirect(
                sceneLyrics.toSceneJson(
                    sceneWidth,
                    sceneHeight,
                    sceneStyle.scaled(renderScale),
                    // Scene dimensions and typography are in downscaled render px.
                    // Passing density in the same space lets Rust evaluate its
                    // landscape dynamic scale in dp, avoiding a second 1.4x boost
                    // on high-density phones.
                    layoutDensity = resources.displayMetrics.density * renderScale,
                    contentTop = resolvedContentTop,
                    contentBottom = contentBottomPx * renderScale,
                    contentLeft = contentLeftPx * renderScale,
                    contentRight = contentRightPx * renderScale,
                    topBar = topBarWire,
                    player = playerWire?.let { player ->
                        player.copy(
                            viewportWidth = player.viewportWidth?.times(renderScale),
                            viewportHeight = player.viewportHeight?.times(renderScale),
                        )
                    },
                )
            )
            if (!sceneApplied) {
                // Preserve the last valid native scene and retry on the next
                // state/size update. Marking this clean used to hide a failed JNI
                // submission indefinitely while the render surface showed black.
                LyricsUiDiagnostics.record(
                    "scene",
                    "setLyricsSceneDirect rejected lines=${sceneLyrics.lines.size} size=${sceneWidth}x$sceneHeight",
                )
                return false
            }
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

        // The native music-foundation clock is deliberately independent of the
        // Kotlin fallback clock. Ask JNI for the time of its last drawn frame so
        // hit-testing still matches the visible rows immediately after a seek.
        val hitTestTimeMs = if (useMusicFoundationClock) -1 else lastRenderedTimeMs
        val lineIndex = engine.hitTestLyricsLine(x * renderScale, y * renderScale, hitTestTimeMs)
        return lineIndex.takeIf { it in sceneLyrics.lines.indices }
    }

    private fun applyCurrentFontConfig() {
        val fontBytes = configuredFontBytes
        val shouldRebindSurface = isAvailable && width > 0 && height > 0

        // NativeTextEngine.configureFonts() replaces the complete EngineState.
        // In particular, that drops AndroidGpuRenderer on the calling thread. If
        // a late Compose resource load changes the font after TextureView has
        // attached, doing that here on the main thread violates EGL's thread
        // affinity and leaves some Samsung drivers unable to create a second
        // window surface. Always tear EGL down on the render thread first.
        if (renderSurface != null || surfaceReady) {
            LyricsUiDiagnostics.record(
                "font",
                "reconfigure: releasing active surface bytes=${fontBytes?.size ?: 0}",
            )
            releaseRenderSurface()
        }

        LyricsUiDiagnostics.record(
            "font",
            "applying bytes=${fontBytes?.size ?: 0} rebind=$shouldRebindSurface",
        )
        // Only the user's primary font; system fonts come from the NDK pool that
        // configureFonts loads, and cosmic-text falls back by the scene's locale.
        engine.configureFonts(
            NativeFontConfig(
                primary = fontBytes?.let { NativeFontSource(bytes = it) } ?: getFontSource(null, context),
                fallbacks = emptyList()
            )
        )
        engineClosed = false
        // configureFonts recreates the native engine (nativeInit), which drops the
        // renderer's mesh-gradient background and playback state — re-apply them.
        applyBackgroundArt()
        queueArtworkPixels.forEach { (key, art) ->
            engine.setQueueArtwork(key, art.pixels, art.width, art.height)
        }
        resetManualScroll()
        // Mark dirty BEFORE (re)binding so bindRenderSurface's ensureScene rebuilds
        // with the new font. configureFonts may have recreated the engine handle,
        // dropping the GPU renderer, so a rebind is required here.
        sceneDirty = true
        if (shouldRebindSurface) {
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
        downX = event.getX(nextPointerIndex)
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
        isPlayerExpansionDragging = false
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
