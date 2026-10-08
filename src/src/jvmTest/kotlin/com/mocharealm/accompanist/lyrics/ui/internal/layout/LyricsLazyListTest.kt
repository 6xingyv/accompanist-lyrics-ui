package com.mocharealm.accompanist.lyrics.ui.internal.layout

import com.mocharealm.accompanist.lyrics.ui.internal.test.TestSceneDispatcher

import com.mocharealm.accompanist.lyrics.ui.composable.list.*
import com.mocharealm.accompanist.lyrics.ui.diagnostics.LyricsSpringTrace
import java.nio.ByteBuffer
import java.nio.ByteOrder

import androidx.compose.animation.core.Easing
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.spring
import androidx.compose.animation.core.tween
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.DragInteraction
import androidx.compose.foundation.layout.*
import androidx.compose.runtime.*
import androidx.compose.runtime.snapshots.Snapshot
import androidx.compose.ui.InternalComposeUiApi
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Canvas
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.input.pointer.PointerEventType
import androidx.compose.ui.input.pointer.PointerType
import androidx.compose.ui.layout.LayoutCoordinates
import androidx.compose.ui.layout.layout
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.layout.positionInRoot
import androidx.compose.ui.platform.FrameRecomposer
import androidx.compose.ui.scene.CanvasLayersComposeScene
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.dp
import kotlin.test.*
import kotlinx.coroutines.*

@OptIn(InternalComposeUiApi::class)
class LyricsLazyListTest {
    @Test
    fun captionReflowAfterLateFocusHandoffPreservesAnchorAcrossRetainedRanges() {
        Host().use { reference -> Host().use { changed ->
            val baseline = LyricsLazyListState()
            val actual = LyricsLazyListState()
            val target = mutableIntStateOf(8)
            var caption by mutableIntStateOf(120)
            var referenceCaption by mutableIntStateOf(120)
            var referenceSettledCaption: State<Int> = mutableStateOf(120)
            var settledCaption: State<Int> = mutableStateOf(120)
            fun items(extra: () -> Int) = List(40) { index -> LyricsListItem(index, 400,
                settledHeightPx = { 280 + extra() }, preserveAnchorOnHeightChange = false) }
            val baselineItems = items { referenceSettledCaption.value }
            val changedItems = items { settledCaption.value }
            reference.content {
                val currentCaption = referenceCaption
                referenceSettledCaption = rememberUpdatedState(currentCaption)
                val previous = remember { intArrayOf(currentCaption) }
                SideEffect {
                    if (previous[0] != currentCaption) {
                        baseline.preserveFollowAnchorForContentChange()
                        previous[0] = currentCaption
                    }
                }
                val follow = lyricsAutoScroll(baseline, { target.intValue }, tween(1200, easing = LinearEasing), 0)
                LyricsLazyColumn(baselineItems, baseline, modifier = follow,
                    beyondBounds = 100.dp, itemSpacing = 0.dp, scrollChain = LyricsScrollChain()) {
                    val extra by androidx.compose.animation.core.animateFloatAsState(
                        currentCaption.toFloat(), com.mocharealm.accompanist.lyrics.ui.internal.effects.LyricsRevealSpring)
                    Box(Modifier.layout { measurable, constraints ->
                        val child = measurable.measure(constraints.copy(minHeight = 0,
                            maxHeight = (280 + extra).toInt()))
                        layout(child.width, (280 + extra).toInt()) { child.place(0, 0) }
                    })
                }
            }
            changed.content {
                val currentCaption = caption
                settledCaption = rememberUpdatedState(currentCaption)
                val previous = remember { intArrayOf(currentCaption) }
                SideEffect {
                    if (previous[0] != currentCaption) {
                        actual.preserveFollowAnchorForContentChange()
                        previous[0] = currentCaption
                    }
                }
                val follow = lyricsAutoScroll(actual, { target.intValue }, tween(1200, easing = LinearEasing), 0)
                LyricsLazyColumn(changedItems, actual, modifier = follow,
                    beyondBounds = 100.dp, itemSpacing = 0.dp, scrollChain = LyricsScrollChain()) {
                    val extra by androidx.compose.animation.core.animateFloatAsState(
                        currentCaption.toFloat(), com.mocharealm.accompanist.lyrics.ui.internal.effects.LyricsRevealSpring)
                    Box(Modifier.layout { measurable, constraints ->
                        val child = measurable.measure(constraints.copy(minHeight = 0,
                            maxHeight = (280 + extra).toInt()))
                        layout(child.width, (280 + extra).toInt()) { child.place(0, 0) }
                    })
                }
            }
            repeat(250) { reference.frame(); changed.frame() }
            caption = 0
            referenceCaption = 0
            repeat(12) { reference.frame(); changed.frame() }
            target.intValue = 9
            repeat(160) { frame ->
                if (frame in listOf(18, 48, 73, 105)) caption = if (caption == 0) 120 else 0
                reference.frame(); changed.frame()
                val expected = baseline.heights.top(9) - baseline.position + baseline.chain.offset(9)
                val observed = actual.heights.top(9) - actual.position + actual.chain.offset(9)
                assertEquals(expected, observed, 2.0,
                    "Caption reflow must retain the focused screen trajectory after handoff; frame=$frame range=${actual.retainedFirst}..${actual.retainedEnd}")
            }
            assertEquals(0.0, actual.heights.top(9) - actual.position + actual.chain.offset(9), 2.0)
            actual.finishCaptionContentChange()
            baseline.finishCaptionContentChange()
            target.intValue = 10
            repeat(200) { reference.frame(); changed.frame() }
            assertFalse(actual.anchoringContentChange, "A settled caption must not take ownership of future follows")
            assertEquals(0.0, actual.heights.top(10) - actual.position + actual.chain.offset(10), 2.0)
        } }
    }

    @Test
    fun captionSettingsDoNotRetargetAnInFlightFollow() {
        Host().use { host ->
            val state = LyricsLazyListState()
            val target = mutableIntStateOf(0)
            var caption by mutableIntStateOf(20)
            var settledCaption: State<Int> = mutableStateOf(20)
            val items = List(30) { index -> LyricsListItem(index, 120,
                settledHeightPx = { 100 + settledCaption.value }, preserveAnchorOnHeightChange = false) }
            host.content {
                val currentCaption = caption
                settledCaption = rememberUpdatedState(currentCaption)
                val previous = remember { intArrayOf(currentCaption) }
                SideEffect {
                    if (previous[0] != currentCaption) {
                        state.preserveFollowAnchorForContentChange()
                        previous[0] = currentCaption
                    }
                }
                val follow = lyricsAutoScroll(state, { target.intValue },
                    tween(2500, easing = LinearEasing), 0)
                LyricsLazyColumn(items, state, modifier = follow, itemSpacing = 0.dp,
                    beyondBounds = 1000.dp, scrollChain = LyricsScrollChain()) {
                    Box(Modifier.height((100 + currentCaption).dp))
                }
            }
            repeat(300) { host.frame() }
            var starts = 0
            val targets = mutableListOf<String>()
            LyricsSpringTrace.install { record ->
                val data = ByteBuffer.wrap(record).order(ByteOrder.LITTLE_ENDIAN)
                if (data.getInt(21) == 112 && data.getInt(25) == 4) {
                    starts++
                    targets += "${data.getDouble(29)} at ${host.millis}"
                }
            }
            try {
                target.intValue = 4
                // Start away from the top clamp so an 80px prefix contraction can be rebased.
                repeat(60) { host.frame() }
                val before = state.heights.top(4) - state.position + state.chain.offset(4)
                repeat(120) { frame ->
                    if (frame % 2 == 0) caption = if (caption == 20) 0 else 20
                    host.frame()
                    assertEquals(1, starts, "Caption toggles must not restart the active scroll: $targets")
                    val actual = state.heights.top(4) - state.position + state.chain.offset(4)
                    val expected = before - (frame + 1) * 480.0 / 2500 * 10
                    assertEquals(expected, actual, 1.0,
                        "Caption geometry must preserve the scroll trajectory at frame=$frame")
                }
                assertEquals(1, starts, "Caption toggles must not restart the active scroll")
            } finally { LyricsSpringTrace.clear() }
        }
    }

    @Test
    fun retargetAdvancesPositionOnEveryFrameWithoutRepeatingTheInitialSample() {
        Host().use { host ->
            val state = LyricsLazyListState()
            val target = mutableIntStateOf(0)
            val items = List(30) { LyricsListItem(it, 100) }
            host.content {
                val follow = lyricsAutoScroll(state, { target.intValue },
                    tween(1000, easing = LinearEasing), 0)
                LyricsLazyColumn(items, state, modifier = follow, itemSpacing = 0.dp,
                    beyondBounds = 1000.dp, scrollChain = LyricsScrollChain()) {
                    Box(Modifier.height(100.dp))
                }
            }
            repeat(10) { host.frame() }
            target.intValue = 2
            repeat(15) { host.frame() }
            var previous = state.position
            host.frame()
            val previousStep = state.position - previous
            assertTrue(previousStep > 1.0)
            target.intValue = 3
            repeat(8) { frame ->
                previous = state.position
                host.frame()
                val step = state.position - previous
                assertTrue(step > previousStep / 2,
                    "Retarget must advance rather than report velocity while repeating t=0; frame=$frame step=$step previous=$previousStep")
            }
        }
    }

    @Test
    fun captionReflowDuringFollowPreservesAnchorTrajectoryAndOnlySpringsBelowIt() {
        Host().use { reference -> Host().use { changed ->
            val referenceState = LyricsLazyListState()
            val changedState = LyricsLazyListState()
            var caption by mutableIntStateOf(20)
            fun items(extra: () -> Int) = List(20) { index ->
                LyricsListItem(index, 120, settledHeightPx = { 100 + extra() },
                    preserveAnchorOnHeightChange = false)
            }
            val referenceItems = items { 20 }
            val changedItems = items { caption }
            reference.content {
                LyricsLazyColumn(referenceItems, referenceState, itemSpacing = 0.dp,
                    beyondBounds = 1000.dp, scrollChain = LyricsScrollChain()) {
                    Box(Modifier.height(120.dp))
                }
            }
            changed.content {
                LyricsLazyColumn(changedItems, changedState, itemSpacing = 0.dp,
                    beyondBounds = 1000.dp, scrollChain = LyricsScrollChain()) {
                    Box(Modifier.height((100 + caption).dp))
                }
            }
            repeat(5) { reference.frame(); changed.frame() }
            reference.scope.launch { referenceState.animateFollowToItem(3, 3, tween(650)) }
            changed.scope.launch { changedState.animateFollowToItem(3, 3, tween(650)) }
            var sawLowerSpring = false
            repeat(110) { frame ->
                if (frame == 15) changedState.preserveFollowAnchorForContentChange()
                if (frame in 15..34) caption = 34 - frame
                reference.frame()
                changed.frame()
                val expected = referenceState.heights.top(3) - referenceState.position + referenceState.chain.offset(3)
                val actual = changedState.heights.top(3) - changedState.position + changedState.chain.offset(3)
                assertEquals(expected, actual, 1.0, "Reflow must preserve the follow trajectory, frame=$frame")
                assertEquals(0f, changedState.chain.offset(3), 0.1f)
                assertEquals(0f, changedState.chain.offset(2), 0.1f)
                if (frame in 15..34 && kotlin.math.abs(changedState.chain.offset(4) - referenceState.chain.offset(4)) > 1f)
                    sawLowerSpring = true
            }
            assertTrue(sawLowerSpring, "Items below the anchor must animate their changed layout")
        } }
    }

    @Test
    fun upwardFollowAnticipatesExpandingVocalsWithoutRetargeting() {
        Host().use { host ->
            val state = LyricsLazyListState()
            var extra by mutableIntStateOf(0)
            val items =
                List(30) { index ->
                    LyricsListItem(
                        index,
                        100,
                        settledHeightPx = { if (index == 2) 200 else 100 },
                        focusOffsetPx = { if (index == 3) 60 else 0 },
                    )
                }
            host.content {
                LyricsLazyColumn(
                    items,
                    state,
                    itemSpacing = 0.dp,
                    beyondBounds = 700.dp,
                    scrollChain = LyricsScrollChain(),
                ) { index ->
                    Box(Modifier.height((100 + if (index == 2) extra else 0).dp))
                }
            }
            host.scope.launch { state.scrollToItem(7) }
            repeat(5) { host.frame() }
            host.scope.launch { state.animateFollowToItem(3, 3, tween(650)) }
            var previous = state.position
            repeat(90) { frame ->
                if (frame in 5..24) extra = (frame - 4) * 5
                host.frame()
                assertTrue(
                    state.position <= previous + 0.5,
                    "Expanding vocals must not reverse upward follow",
                )
                previous = state.position
            }
            assertEquals(460.0, state.position, 1.0)
        }
    }

    @Test
    fun smallScrollOnlyPlacesUntilRangeOrHeightChanges() {
        Host().use { host ->
            val state = LyricsLazyListState()
            val items = List(100) { LyricsListItem(it, 100) }
            var expanded by mutableStateOf(false)
            val tops = mutableMapOf<Int, Float>()
            host.content {
                LyricsLazyColumn(items, state, itemSpacing = 0.dp, beyondBounds = 150.dp) { index ->
                    Box(
                        Modifier.height(if (index == 3 && expanded) 130.dp else 100.dp)
                            .onGloballyPositioned { tops[index] = it.positionInRoot().y }
                    )
                }
            }
            repeat(5) { host.frame() }
            val passes = state.measurePasses
            val initialTop = tops.getValue(3)
            repeat(10) {
                state.dispatchRawDelta(1f)
                host.frame()
            }
            assertEquals(
                passes,
                state.measurePasses,
                "Cached small scroll must not run measure policy",
            )
            assertEquals(initialTop - 10f, tops.getValue(3))
            assertEquals(10, state.firstVisibleItemScrollOffset)
            expanded = true
            repeat(3) { host.frame() }
            assertTrue(state.measurePasses > passes)
            assertEquals(130, state.heights.height(3))
            val afterHeight = state.measurePasses
            state.dispatchRawDelta(1000f)
            repeat(3) { host.frame() }
            assertTrue(state.measurePasses > afterHeight)
            assertTrue(state.firstVisibleItemIndex >= 9)
        }
    }

    @Test
    fun playbackLoopReturnsFromLastLineToFirstWithoutAnotherFocusEvent() {
        Host().use { host ->
            val state = LyricsLazyListState()
            val time = mutableIntStateOf(30000)
            val items = List(30) { LyricsListItem(it, 100) }
            host.content {
                val follow =
                    lyricsAutoScroll(
                        state,
                        { if (time.intValue >= 20000) 20 else 0 },
                        tween(200),
                        2500,
                        playbackPosition = { time.intValue },
                    )
                LyricsLazyColumn(
                    items,
                    state,
                    modifier = follow,
                    itemSpacing = 0.dp,
                    scrollChain = LyricsScrollChain(),
                ) {
                    Box(Modifier.height(100.dp))
                }
            }
            repeat(150) { host.frame() }
            assertEquals(2000.0, state.position, 1.0)
            time.intValue = 0
            repeat(150) { host.frame() }
            assertEquals(0.0, state.position, 1.0)
            assertEquals(0, state.firstVisibleItemIndex)
        }
    }

    @Test
    fun upwardSpringUsesPixelThresholdAtCompletion() {
        Host().use { host ->
            val state = LyricsLazyListState()
            val items = List(100) { LyricsListItem(it, 100) }
            host.content {
                LyricsLazyColumn(items, state, itemSpacing = 0.dp) { Box(Modifier.height(100.dp)) }
            }
            host.scope.launch { state.scrollToItem(50) }
            repeat(3) { host.frame() }
            val job =
                host.scope.launch {
                    state.animateScrollToItem(
                        0,
                        animationSpec =
                            spring(dampingRatio = 1f, stiffness = 100f, visibilityThreshold = 0.1f),
                    )
                }
            var lastStep = 0.0
            repeat(500) {
                if (job.isActive) {
                    val before = state.position
                    host.frame()
                    lastStep = kotlin.math.abs(state.position - before)
                }
            }
            assertTrue(job.isCompleted)
            assertTrue(
                lastStep < 1.0,
                "Spring completion must not snap a normalized fraction of a long scroll: $lastStep",
            )
        }
    }

    @Test
    fun accompanimentHeightChangesStartAndPreserveSpringTail() {
        Host().use { host ->
            val state = LyricsLazyListState()
            val items = List(30) { LyricsListItem(it, 100) }
            var extra by mutableIntStateOf(0)
            host.content {
                LyricsLazyColumn(
                    items,
                    state,
                    itemSpacing = 0.dp,
                    scrollChain = LyricsScrollChain(),
                ) { index ->
                    Box(Modifier.height((100 + if (index == 1) extra else 0).dp))
                }
            }
            repeat(3) { host.frame() }
            extra = 80
            host.frame()
            assertTrue(state.chain.active)
            assertEquals(
                -80f,
                state.chain.offset(2),
                2f,
                "Height changes must preserve the next item's visible position",
            )
            repeat(15) { host.frame() }
            val oldOffset = state.chain.offset(2)
            extra = 0
            host.frame()
            assertTrue(state.chain.active)
            assertTrue(
                state.chain.offset(2) > oldOffset + 60f,
                "Exit must retarget the existing spring, not clear it",
            )
            repeat(1000) { host.frame() }
            assertFalse(state.chain.active)
        }
    }

    @Test
    fun collapsingItemDuringPredictedFollowDoesNotDoubleMoveFollowingItem() {
        Host().use { host ->
            val state = LyricsLazyListState()
            val target = mutableIntStateOf(0)
            var extra by mutableIntStateOf(80)
            val tops = mutableMapOf<Int, Float>()
            val items =
                List(10) {
                    LyricsListItem(
                        it,
                        100,
                        // The first item is currently 80px taller; this is its settled aggregate
                        // height that predicted follow uses for the destination.
                        settledHeightPx = { 100 },
                        preserveAnchorOnHeightChange = false,
                    )
                }
            host.content {
                val follow =
                    lyricsAutoScroll(
                        state,
                        { target.intValue },
                        tween(600, easing = LinearEasing),
                        0,
                    )
                LyricsLazyColumn(
                    items,
                    state,
                    modifier = follow,
                    itemSpacing = 0.dp,
                    beyondBounds = 500.dp,
                    scrollChain = LyricsScrollChain(),
                ) { index ->
                    Box(
                        Modifier.height((100 + if (index == 0) extra else 0).dp)
                            .onGloballyPositioned { tops[index] = it.positionInRoot().y },
                    )
                }
            }
            repeat(5) { host.frame() }
            target.intValue = 1
            val samples = mutableListOf<Float>()
            repeat(60) { frame ->
                extra = (80 - frame * 3).coerceAtLeast(0)
                host.frame()
                tops[1]?.let { samples.add(it) }
            }
            repeat(80) { host.frame() }
            assertEquals(
                100.0,
                state.position,
                1.0,
                "position=${state.position}, samples=$samples",
            )
            assertTrue(
                samples.minOrNull() ?: Float.POSITIVE_INFINITY >= -1f,
                "Following item must not cross its settled target while the preceding item collapses: $samples",
            )
        }
    }

    @Test
    fun accompanimentGeometryRetargetPreservesScrollVelocityOnRevealAndHide() {
        Host().use { host ->
            val state = LyricsLazyListState()
            val target = mutableIntStateOf(4)
            var settledAccompanimentHeight by mutableIntStateOf(0)
            var measuredAccompanimentHeight by mutableIntStateOf(0)
            val items =
                List(30) { index ->
                    LyricsListItem(
                        index,
                        100,
                        settledHeightPx = {
                            100 + if (index == 1) settledAccompanimentHeight else 0
                        },
                        preserveAnchorOnHeightChange = false,
                    )
                }
            host.content {
                val follow =
                    lyricsAutoScroll(
                        state,
                        { target.intValue },
                        tween(600, easing = LinearEasing),
                        0,
                    )
                LyricsLazyColumn(
                    items,
                    state,
                    modifier = follow,
                    itemSpacing = 0.dp,
                    beyondBounds = 1000.dp,
                    scrollChain = LyricsScrollChain(),
                ) { index ->
                    Box(
                        Modifier.height(
                            (100 + if (index == 1) measuredAccompanimentHeight else 0).dp,
                        ),
                    )
                }
            }
            repeat(5) { host.frame() }
            var previousPosition = state.position
            val velocities = mutableListOf<Double>()
            repeat(10) {
                host.frame()
                velocities += state.position - previousPosition
                previousPosition = state.position
            }
            val velocityBeforeReveal = velocities.last()

            // Playback predicts the full accompaniment height before its rows finish appearing.
            settledAccompanimentHeight = 40
            host.frame()
            velocities += state.position - previousPosition
            previousPosition = state.position
            repeat(6) { frame ->
                measuredAccompanimentHeight = ((frame + 1) * 40 / 6)
                host.frame()
                velocities += state.position - previousPosition
                previousPosition = state.position
            }
            val velocityAfterReveal = velocities[10]
            assertTrue(
                velocityAfterReveal > 0.0,
                "A revealing accompaniment must not reverse or restart follow motion: $velocities",
            )
            assertTrue(
                kotlin.math.abs(velocityAfterReveal - velocityBeforeReveal) < 10.0,
                "Reveal retarget must keep physical scroll velocity: $velocities",
            )

            settledAccompanimentHeight = 0
            host.frame()
            velocities += state.position - previousPosition
            previousPosition = state.position
            repeat(6) { frame ->
                measuredAccompanimentHeight = (40 - (frame + 1) * 40 / 6).coerceAtLeast(0)
                host.frame()
                velocities += state.position - previousPosition
                previousPosition = state.position
            }
            val velocityAfterHide = velocities.last()
            assertTrue(
                velocityAfterHide >= -0.5,
                "A hiding accompaniment must retain momentum while its target moves: $velocities",
            )
            repeat(80) { host.frame() }
            assertEquals(400.0, state.position, 1.0)
        }
    }

    @Test
    fun lyricClickKeepsSpringTailAcrossPlaybackSeekAndTimelineEnd() {
        Host().use { host ->
            val state = LyricsLazyListState()
            val items = List(30) { LyricsListItem(it, 100) }
            val target = mutableIntStateOf(0)
            val time = mutableIntStateOf(0)
            host.content {
                val follow =
                    lyricsAutoScroll(
                        state,
                        { target.intValue },
                        tween(200, easing = LinearEasing),
                        0,
                        playbackPosition = { time.intValue },
                    )
                LyricsLazyColumn(
                    items,
                    state,
                    modifier = follow,
                    itemSpacing = 0.dp,
                    scrollChain = LyricsScrollChain(),
                ) {
                    Box(Modifier.height(100.dp))
                }
            }
            repeat(3) { host.frame() }
            target.intValue = 1
            repeat(12) { host.frame() }
            val before = state.chain.offset(4)
            assertTrue(before > 0f)
            state.resumeAutoScroll(10000)
            assertEquals(
                before,
                state.chain.offset(4),
                "Click must not discard visible spring displacement",
            )
            target.intValue = 3
            time.intValue = 10000
            repeat(25) { host.frame() }
            assertEquals(300.0, state.position, 1.0)
            assertTrue(
                state.chain.active,
                "The scroll timeline ending must leave the spring tail running",
            )
            assertTrue(
                state.chain.offset(5) > state.chain.offset(4),
                "Click seeks retain neighbour coupling",
            )
            val tail = state.chain.offset(5)
            state.dispatchRawDelta(0f)
            assertEquals(
                tail,
                state.chain.offset(5),
                "A no-op scroll must not clear the spring tail",
            )
            time.intValue = 9990 // Player correction while the tail is still moving.
            host.frame()
            assertTrue(
                state.chain.offset(5) > tail * 0.7f,
                "Playback corrections must not snap the tail",
            )
            repeat(1000) { host.frame() }
            assertFalse(state.chain.active)
        }
    }

    @Test
    fun consecutiveLyricClicksRetargetTheInFlightJump() {
        Host().use { host ->
            val state = LyricsLazyListState()
            val items = List(30) { LyricsListItem(it, 100) }
            host.content {
                val follow =
                    lyricsAutoScroll(
                        state,
                        { 0 },
                        tween(650, easing = LinearEasing),
                        0,
                        playbackPosition = { 0 },
                    )
                LyricsLazyColumn(
                    items,
                    state,
                    modifier = follow,
                    itemSpacing = 0.dp,
                    scrollChain = LyricsScrollChain(),
                ) {
                    Box(Modifier.height(100.dp))
                }
            }
            repeat(3) { host.frame() }

            var previousPosition = state.position
            for (index in 1..12) {
                val seek = index * 1000
                state.resumeAutoScroll(seek, index)
                host.frame()
                assertTrue(
                    state.position >= previousPosition,
                    "A later click must continue the in-flight jump toward item $index",
                )
                previousPosition = state.position
            }

            repeat(30) { host.frame() }
            assertTrue(state.position > 100.0, "Rapid retargeting must keep the jump moving")
            assertTrue(state.position < 1200.0, "Retarget must retain the configured duration")
            repeat(40) { host.frame() }
            assertEquals(1200.0, state.position, 1.0, "The latest clicked item must win")
        }
    }

    @Test
    fun clickingSameLyricRepeatedlyAfterRetargetDoesNotResetOtherItemSprings() {
        Host().use { host ->
            val state = LyricsLazyListState()
            val target = mutableIntStateOf(0)
            val time = mutableIntStateOf(0)
            var clicked = -1
            val onLineClicked: (Int) -> Unit = { index ->
                clicked = index
                state.resumeAutoScroll(index * 1000, index)
                time.intValue = index * 1000
                target.intValue = index
            }
            host.content {
                val follow =
                    lyricsAutoScroll(
                        state,
                        { target.intValue },
                        tween(200, easing = LinearEasing),
                        0,
                        playbackPosition = { time.intValue },
                    )
                LyricsLazyColumn(
                    List(50) { LyricsListItem(it, 100) },
                    state,
                    modifier = follow,
                    itemSpacing = 0.dp,
                    scrollChain = LyricsScrollChain(),
                ) { index ->
                    Box(
                        Modifier.fillMaxWidth()
                            .height(100.dp)
                            .clickable { onLineClicked(index) }
                    )
                }
            }
            repeat(3) { host.frame() }

            target.intValue = 1
            time.intValue = 16
            repeat(5) { host.frame() }
            assertTrue(state.position in 0.0..100.0)
            var previousPosition = state.position
            repeat(6) {
                onLineClicked(5)
                host.frame()
                assertTrue(
                    state.position >= previousPosition - 0.5,
                    "Repeated clicks on one row must preserve follow motion: " +
                        "$previousPosition -> ${state.position}",
                )
                previousPosition = state.position
            }
            repeat(60) { host.frame() }
            assertEquals(500.0, state.position, 1.0)
            assertTrue(state.chain.offset(8) > 0.1f)
            assertTrue(state.chain.active)

            repeat(6) {
                onLineClicked(5)
                host.frame()
                assertEquals(5, clicked)
                assertTrue(
                    state.chain.offset(8) > 0.1f,
                    "Clicking the focused line must not snap another line's spring to rest",
                )
                assertTrue(state.chain.active)
            }
        }
    }

    @Test
    fun stalePlaybackSamplesAfterLyricClicksDoNotCollapseOtherItemSprings() {
        fun tailOffsetAfterSameDuration(repeatClickAndStaleSample: Boolean): Float {
            return Host().use { host ->
                val state = LyricsLazyListState()
                val target = mutableIntStateOf(0)
                val time = mutableIntStateOf(0)
                host.content {
                    val follow =
                        lyricsAutoScroll(
                            state,
                            { target.intValue },
                            tween(200, easing = LinearEasing),
                            0,
                            playbackPosition = { time.intValue },
                        )
                    LyricsLazyColumn(
                        List(30) { LyricsListItem(it, 100) },
                        state,
                        modifier = follow,
                        itemSpacing = 0.dp,
                        scrollChain = LyricsScrollChain(),
                    ) {
                        Box(Modifier.height(100.dp))
                    }
                }
                repeat(3) { host.frame() }
                target.intValue = 1
                time.intValue = 16
                repeat(25) { host.frame() }
                val tailBefore = state.chain.offset(4)
                assertTrue(tailBefore > 0.1f)
                assertTrue(state.chain.active)

                if (repeatClickAndStaleSample) {
                    repeat(6) {
                        // Mirrors KaraokeLyricsView's order: register follow before the player
                        // publishes the new seek position. The UI state may first publish the
                        // expected value, followed by a stale controller sample.
                        state.resumeAutoScroll(seekPosition = 10_000, targetIndex = 1)
                        host.frame()
                        time.intValue = 10_000
                        host.frame()
                        time.intValue = 9_000
                        host.frame()
                    }
                } else {
                    repeat(18) {
                        time.intValue += 16
                        host.frame()
                    }
                }
                state.chain.offset(4)
            }
        }

        val naturalTail = tailOffsetAfterSameDuration(repeatClickAndStaleSample = false)
        val clickedTail = tailOffsetAfterSameDuration(repeatClickAndStaleSample = true)
        assertTrue(
            clickedTail > naturalTail * 0.8f,
            "Repeated same-row clicks must preserve adjacent spring motion: " +
                "natural=$naturalTail clicked=$clickedTail",
        )
    }

    @Test
    fun clickNearFollowCompletionRetargetsBeforeThePreviousDestination() {
        Host().use { host ->
            val state = LyricsLazyListState()
            val items = List(30) { LyricsListItem(it, 100) }
            host.content {
                val follow =
                    lyricsAutoScroll(
                        state,
                        { 0 },
                        tween(200, easing = LinearEasing),
                        0,
                    )
                LyricsLazyColumn(items, state, modifier = follow, itemSpacing = 0.dp) {
                    Box(Modifier.height(100.dp))
                }
            }
            repeat(3) { host.frame() }

            state.resumeAutoScroll(5000, 5)
            repeat(11) { host.frame() }
            assertTrue(state.position < 500.0)
            assertNotNull(state.onImmediateFollowRequest)

            state.resumeAutoScroll(6000, 6)
            host.frame()
            val firstFramePosition = state.position
            host.frame()
            assertEquals(6, state.followAnchorIndex)
            assertTrue(
                state.position > firstFramePosition + 15.0 && state.position < 490.0,
                "A queued click near completion must redirect the in-flight scroll, " +
                    "positions=$firstFramePosition -> ${state.position}",
            )
            repeat(60) { host.frame() }
            assertEquals(600.0, state.position, 1.0)
        }
    }

    @Test
    fun springTailMovesOnlyLayersAndSeeksPreserveMotion() {
        Host().use { host ->
            val state = LyricsLazyListState()
            val items = List(100) { LyricsListItem(it, 100) }
            val target = mutableIntStateOf(0)
            val time = mutableIntStateOf(0)
            val coordinates = mutableMapOf<Int, LayoutCoordinates>()
            var measures = 0
            var compositions = 0
            var clicked = -1
            host.content {
                SideEffect { compositions++ }
                val follow =
                    lyricsAutoScroll(
                        state,
                        { target.intValue },
                        tween(200, easing = LinearEasing),
                        0,
                        playbackPosition = { time.intValue },
                    )
                LyricsLazyColumn(
                    items,
                    state,
                    modifier = follow,
                    itemSpacing = 0.dp,
                    scrollChain = LyricsScrollChain(),
                ) { index ->
                    Box(
                        Modifier.fillMaxWidth()
                            .height(100.dp)
                            .clickable { clicked = index }
                            .layout { measurable, constraints ->
                                measures++
                                val p = measurable.measure(constraints)
                                layout(p.width, p.height) { p.place(0, 0) }
                            }
                            .onGloballyPositioned { coordinates[index] = it }
                    )
                }
            }
            repeat(3) { host.frame() }
            target.intValue = 1
            time.intValue = 16
            repeat(25) { host.frame() }
            assertEquals(100.0, state.position, 1.0, "Focus still obeys the 200ms scroll duration")
            assertTrue(state.chain.active)
            assertTrue(state.chain.offset(3) > state.chain.offset(2))
            val y = coordinates.getValue(3).positionInRoot().y
            val previousMeasures = measures
            val previousCompositions = compositions
            repeat(8) { host.frame() }
            assertTrue(
                coordinates.getValue(3).positionInRoot().y < y,
                "Actual item transform must move",
            )
            assertEquals(previousMeasures, measures, "Tail settling must not remeasure the list")
            assertEquals(previousCompositions, compositions)
            val hitPosition = Offset(200f, coordinates.getValue(3).positionInRoot().y + 50f)
            host.scene.sendPointerEvent(
                PointerEventType.Press,
                hitPosition,
                timeMillis = host.millis,
                type = PointerType.Touch,
            )
            host.frame()
            host.scene.sendPointerEvent(
                PointerEventType.Release,
                hitPosition,
                timeMillis = host.millis,
                type = PointerType.Touch,
            )
            host.frame()
            assertEquals(3, clicked, "Hit testing must follow the spring-transformed item")
            val tailBeforeDrag = state.chain.offset(3)
            assertTrue(tailBeforeDrag > 0f, "Fixture must still have a moving spring tail")
            val drag = DragInteraction.Start()
            host.scope.launch { state.interactionSource.emit(drag) }
            repeat(3) { host.frame() }
            assertTrue(state.isManualScrolling, "Dragging suspends automatic following")
            assertTrue(state.chain.active, "Dragging preserves independently settling row springs")
            assertTrue(state.chain.offset(3) in 0f..tailBeforeDrag && state.chain.offset(3) > 0f,
                "Spring tail must continue settling instead of being reset to zero")
            val manualPosition = state.position
            val tailDuringDrag = state.chain.offset(3)
            repeat(5) { host.frame() }
            assertEquals(manualPosition, state.position, 0.001, "Automatic scroll stays suspended")
            assertTrue(state.chain.offset(3) < tailDuringDrag, "Spring motion continues while dragging")
            host.scope.launch { state.interactionSource.emit(DragInteraction.Stop(drag)) }
            repeat(30) { host.frame() }
            target.intValue = 5
            time.intValue = 10000 // Discontinuous seek: glide as a uniform block.
            repeat(10) { host.frame() }
            assertTrue(state.chain.active, "A seek must glide from the visible spring positions")
            assertTrue(state.chain.offset(6) > 0f)
            repeat(20) { host.frame() }
            assertEquals(500.0, state.position, 1.0)
            target.intValue = 6
            time.intValue = 10016
            repeat(6) { host.frame() }
            target.intValue = 7
            time.intValue = 10032
            repeat(1000) { host.frame() }
            assertEquals(700.0, state.position, 1.0)
            assertFalse(
                state.chain.active,
                "Rapid focus changes must eventually stop requesting frames",
            )
        }
    }

    @Test
    fun touchDragAndWheelScrollInExpectedDirectionWithoutRecomposingOwner() {
        Host().use { host ->
            val state = LyricsLazyListState()
            val items = List(100) { LyricsListItem(it, 100) }
            var compositions = 0
            host.content {
                SideEffect { compositions++ }
                LyricsLazyColumn(items, state, itemSpacing = 0.dp) { Box(Modifier.height(100.dp)) }
            }
            val before = compositions
            fun pointer(type: PointerEventType, y: Float) {
                host.scene.sendPointerEvent(
                    type,
                    Offset(200f, y),
                    timeMillis = host.millis,
                    type = PointerType.Touch,
                )
                host.frame(16)
            }
            pointer(PointerEventType.Press, 450f)
            pointer(PointerEventType.Move, 400f)
            pointer(PointerEventType.Move, 300f)
            pointer(PointerEventType.Move, 200f)
            assertTrue(state.position > 100, "Upward finger movement must reveal later items")
            pointer(PointerEventType.Release, 200f)
            host.scope.launch { state.scrollToItem(0) }
            repeat(3) { host.frame() }
            host.scene.sendPointerEvent(
                PointerEventType.Scroll,
                Offset(200f, 200f),
                scrollDelta = Offset(0f, 3f),
                timeMillis = host.millis,
            )
            repeat(30) { host.frame() }
            assertTrue(state.position > 0, "Wheel-down must reveal later items")
            assertEquals(before, compositions)
        }
    }

    private class Host : AutoCloseable {
        val dispatcher = TestSceneDispatcher()
        val recomposer = FrameRecomposer(dispatcher)
        val scene = CanvasLayersComposeScene(recomposer, size = IntSize(400, 600))
        val canvas = Canvas(ImageBitmap(400, 600))
        var millis = 0L
        lateinit var scope: CoroutineScope

        fun content(content: @Composable () -> Unit) {
            scene.setContent {
                scope = rememberCoroutineScope()
                content()
            }
            frame()
        }

        fun frame(step: Long = 10L) {
            millis += step
            dispatcher.runCurrent()
            Snapshot.sendApplyNotifications()
            recomposer.performFrame(millis * 1_000_000)
            scene.measureAndLayout()
            scene.draw(canvas)
        }

        override fun close() {
            scene.close()
            recomposer.close()
            dispatcher.runCurrent()
        }
    }

    @Test
    fun heightIndexHandlesUpdatesZeroSizesAndLargeLists() {
        val heights = IntArray(10000) { if (it % 5 == 0) 0 else 31 }
        val index = LyricsHeightIndex(heights, 7)
        var top = 0.0
        for (i in heights.indices) {
            assertEquals(top, index.top(i))
            if (heights[i] > 0) assertEquals(i, index.itemAt(top + 1))
            top += heights[i] + if (i < heights.lastIndex) 7 else 0
        }
        assertEquals(top, index.total)
        index.update(5, 100)
        assertEquals(top + 100, index.total)
        repeat(100) { index.update(5, 100) }
        assertEquals(top + 100, index.total)
        index.update(5, 0)
        assertEquals(top, index.total)
        index.update(heights.lastIndex, Int.MAX_VALUE)
        assertEquals(top - heights.last() + Int.MAX_VALUE, index.total)
        index.update(heights.lastIndex, heights.last())
        assertEquals(top, index.total)
        assertEquals(0, LyricsHeightIndex(intArrayOf(), 0).itemAt(10.0))
    }

    @Test
    fun spacingPaddingAndFarSeeksRemainLazy() {
        Host().use { host ->
            val state = LyricsLazyListState()
            val items = List(10000) { LyricsListItem("item-$it", 100) }
            val composed = mutableSetOf<Int>()
            val tops = mutableMapOf<Int, Float>()
            host.content {
                LyricsLazyColumn(
                    items,
                    state,
                    itemSpacing = 23.dp,
                    contentPadding = PaddingValues(top = 32.dp, bottom = 600.dp),
                ) { index ->
                    SideEffect { composed.add(index) }
                    Box(
                        Modifier.height(100.dp).onGloballyPositioned {
                            tops[index] = it.positionInRoot().y
                        }
                    )
                }
            }
            assertEquals(32f, tops[0])
            assertEquals(123f, tops.getValue(1) - tops.getValue(0))
            assertTrue(composed.size < 12)
            host.scope.launch { state.scrollToItem(9000) }
            repeat(3) { host.frame() }
            assertEquals(9000, state.firstVisibleItemIndex)
            assertEquals(32f, tops[9000], "Padding must be applied once")
            assertTrue(composed.size < 25, "Far seek must not compose intervening items")
        }
    }

    private fun positionAfterHalf(easing: Easing): Double {
        Host().use { host ->
            val state = LyricsLazyListState()
            val items = List(30) { LyricsListItem(it, 100) }
            host.content {
                LyricsLazyColumn(items, state, itemSpacing = 0.dp) { Box(Modifier.height(100.dp)) }
            }
            val job =
                host.scope.launch {
                    state.animateScrollToItem(10, animationSpec = tween(1000, easing = easing))
                }
            host.frame() // initial animation frame
            repeat(50) { host.frame() }
            val halfway = state.position
            assertTrue(job.isActive)
            repeat(50) { host.frame() }
            assertTrue(job.isCompleted)
            assertEquals(1000.0, state.position, 1.0)
            return halfway
        }
    }

    @Test
    fun requestedDurationAndEasingControlActualScroll() {
        assertEquals(500.0, positionAfterHalf(LinearEasing), 12.0)
        assertEquals(250.0, positionAfterHalf(Easing { it * it }), 12.0)
    }

    @Test
    fun manualBrowsingRemainsClearUntilResumeDelayOrExplicitResume() {
        Host().use { host ->
            val state = LyricsLazyListState()
            val items = List(10) { LyricsListItem(it, 100) }
            host.content {
                val follow = lyricsAutoScroll(state, { 0 }, tween(100), 100)
                LyricsLazyColumn(items, state, modifier = follow) { Box(Modifier.height(100.dp)) }
            }
            repeat(3) { host.frame() }
            val drag = DragInteraction.Start()
            host.scope.launch { state.interactionSource.emit(drag) }
            repeat(3) { host.frame() }
            assertTrue(state.isManualScrolling)
            host.scope.launch { state.interactionSource.emit(DragInteraction.Stop(drag)) }
            host.frame()
            assertTrue(state.isManualScrolling, "Releasing must retain clear text during the delay")
            // Snapshot collectors and the delay continuation also need owner-thread frames.
            // Waiting without pumping frames can postpone the start of the resume timer.
            val deadline = System.nanoTime() + 2_000_000_000L
            while (state.isManualScrolling && System.nanoTime() < deadline) {
                runBlocking { delay(10) }
                host.frame()
            }
            assertFalse(state.isManualScrolling, "Following resumes after the release delay")
            val secondDrag = DragInteraction.Start()
            host.scope.launch { state.interactionSource.emit(secondDrag) }
            host.frame()
            assertTrue(state.isManualScrolling)
            host.scope.launch { state.interactionSource.emit(DragInteraction.Stop(secondDrag)) }
            host.frame()
            state.resumeAutoScroll()
            repeat(3) { host.frame() }
            assertFalse(
                state.isManualScrolling,
                "A lyric click resumes blur and following together",
            )
        }
    }

    @Test
    fun latestTargetInterruptsAndManualReleaseResumesSameTarget() {
        Host().use { host ->
            val state = LyricsLazyListState()
            val target = mutableIntStateOf(8)
            val items = List(30) { LyricsListItem(it, 100) }
            host.content {
                val follow =
                    lyricsAutoScroll(
                        state,
                        { target.intValue },
                        tween(200, easing = LinearEasing),
                        0,
                    )
                LyricsLazyColumn(items, state, modifier = follow, itemSpacing = 0.dp) {
                    Box(Modifier.height(100.dp))
                }
            }
            repeat(8) { host.frame() }
            target.intValue = 2
            repeat(25) { host.frame() }
            assertEquals(200.0, state.position, 1.0)
            val drag = DragInteraction.Start()
            host.scope.launch { state.interactionSource.emit(drag) }
            repeat(2) { host.frame() }
            host.scope.launch { state.scrollToItem(5) }
            repeat(25) { host.frame() }
            assertEquals(
                500.0,
                state.position,
                1.0,
                "Following must be suspended during manual browsing",
            )
            host.scope.launch { state.interactionSource.emit(DragInteraction.Stop(drag)) }
            repeat(25) { host.frame() }
            assertEquals(
                200.0,
                state.position,
                1.0,
                "Follow must resume without needing a new focus event",
            )
        }
    }

    @Test
    fun reorderPreservesStableKeyAndEmptyListIsScrollableSafely() {
        Host().use { host ->
            val state = LyricsLazyListState()
            var items by mutableStateOf(List(30) { LyricsListItem(it, 100) })
            host.content {
                LyricsLazyColumn(items, state, itemSpacing = 0.dp) { Box(Modifier.height(100.dp)) }
            }
            host.scope.launch { state.scrollToItem(10, 25) }
            repeat(3) { host.frame() }
            items = listOf(LyricsListItem(-1, 100)) + items
            repeat(3) { host.frame() }
            assertEquals(11, state.firstVisibleItemIndex)
            assertEquals(25, state.firstVisibleItemScrollOffset)
            items = emptyList()
            repeat(3) { host.frame() }
            val job = host.scope.launch { state.animateScrollToItem(0) }
            host.frame()
            assertTrue(job.isCompleted)
            assertEquals(0.0, state.position)
        }
    }

    @Test
    fun expandingItemAboveViewportPreservesVisibleAnchor() {
        Host().use { host ->
            val state = LyricsLazyListState()
            val items = List(30) { LyricsListItem(it, 100) }
            var expanded by mutableStateOf(false)
            val tops = mutableMapOf<Int, Float>()
            host.content {
                LyricsLazyColumn(items, state, itemSpacing = 0.dp, beyondBounds = 200.dp) { index ->
                    Box(
                        Modifier.height(if (index == 4 && expanded) 180.dp else 100.dp)
                            .onGloballyPositioned { tops[index] = it.positionInRoot().y }
                    )
                }
            }
            host.scope.launch { state.scrollToItem(5, 20) }
            repeat(3) { host.frame() }
            val before = tops.getValue(5)
            expanded = true
            repeat(3) { host.frame() }
            assertEquals(before, tops[5])
            assertEquals(5, state.firstVisibleItemIndex)
            assertEquals(20, state.firstVisibleItemScrollOffset)
        }
    }

    @Test
    fun changingSpacingRetainsAnchorAndUpdatesActualGap() {
        Host().use { host ->
            val state = LyricsLazyListState()
            val items = List(30) { LyricsListItem(it, 100) }
            var spacing by mutableStateOf(0.dp)
            val tops = mutableMapOf<Int, Float>()
            host.content {
                LyricsLazyColumn(items, state, itemSpacing = spacing) { index ->
                    Box(
                        Modifier.height(100.dp).onGloballyPositioned {
                            tops[index] = it.positionInRoot().y
                        }
                    )
                }
            }
            host.scope.launch { state.scrollToItem(5) }
            repeat(3) { host.frame() }
            spacing = 30.dp
            repeat(3) { host.frame() }
            assertEquals(5, state.firstVisibleItemIndex)
            assertEquals(0f, tops[5])
            assertEquals(130f, tops.getValue(6) - tops.getValue(5))
        }
    }
}
