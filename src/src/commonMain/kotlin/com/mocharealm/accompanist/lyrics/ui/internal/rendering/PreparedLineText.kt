package com.mocharealm.accompanist.lyrics.ui.internal.rendering

import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.*
import androidx.compose.runtime.*
import androidx.compose.ui.AbsoluteAlignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawWithCache
import androidx.compose.ui.graphics.*
import androidx.compose.ui.graphics.drawscope.*
import androidx.compose.ui.layout.layout
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.text.drawText
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.constrainHeight
import androidx.compose.ui.unit.dp
import com.mocharealm.accompanist.lyrics.core.model.karaoke.KaraokeLine
import com.mocharealm.accompanist.lyrics.ui.internal.effects.LyricsReveal
import com.mocharealm.accompanist.lyrics.ui.internal.effects.LyricsRevealProgress
import com.mocharealm.accompanist.lyrics.ui.internal.effects.LyricsRevealSpring
import com.mocharealm.accompanist.lyrics.ui.internal.effects.rememberLineVariantTransition
import com.mocharealm.accompanist.lyrics.ui.internal.diagnostics.traceLyrics
import com.mocharealm.accompanist.lyrics.ui.internal.diagnostics.lyricsTraceEnabled
import com.mocharealm.accompanist.lyrics.ui.internal.diagnostics.setLyricsTraceCounter
import com.mocharealm.accompanist.lyrics.ui.internal.playback.LyricsPlaybackState
import com.mocharealm.accompanist.lyrics.ui.preparation.LineVariantBinding
import com.mocharealm.accompanist.lyrics.ui.preparation.PreparedLine
import com.mocharealm.accompanist.lyrics.ui.preparation.PreparedRow

/** Everything a band needs that does not depend on which variant owns it. */
private class BandRenderEnv(
    val playback: LyricsPlaybackState,
    val resources: LyricsRenderResources,
    val currentTimeProvider: () -> Int,
    val color: Color,
    val paints: RowPaints,
    val debug: Boolean,
    val rightAligned: Boolean,
    val lineWidth: Float,
    val binding: LineVariantBinding?,
    /** 1 exposes the aligned geometry, 0 the compact one. */
    val alignedness: () -> Float,
)

@Composable
internal fun PreparedLineText(
    prepared: PreparedLine,
    playback: LyricsPlaybackState,
    resources: LyricsRenderResources,
    currentTimeProvider: () -> Int,
    modifier: Modifier = Modifier,
    verticalPadding: Dp = 8.dp,
    showTranslation: Boolean = true,
    showPhonetic: Boolean = true,
    showDebugRectangles: Boolean = false,
) {
    val currentTime by rememberUpdatedState(currentTimeProvider)
    val density = LocalDensity.current
    val activeColor = resources.color
    val paints = remember(activeColor) { RowPaints(activeColor) }
    val transition = rememberLineVariantTransition(prepared, showPhonetic)
    val compact = prepared.compact
    val currentAligned = transition?.currentAligned ?: true
    val targetAligned = transition?.targetAligned ?: true
    val blending = transition != null && currentAligned != targetAligned
    val alignedness: () -> Float =
        transition?.let { { it.progress.value } } ?: { if (showPhonetic) 1f else 0f }
    val currentLine = if (currentAligned) prepared else compact ?: prepared
    val targetLine = if (targetAligned) prepared else compact ?: prepared
    val alignedRaster = resources.rasterState(prepared)
    val compactRaster = compact?.let { resources.rasterState(it) }
    DisposableEffect(resources, prepared, compact) {
        resources.retain(prepared)
        compact?.let(resources::retain)
        onDispose {
            resources.release(prepared)
            compact?.let(resources::release)
        }
    }
    LaunchedEffect(resources, currentLine, targetLine) {
        resources.prepareRaster(currentLine)
        if (targetLine !== currentLine) resources.prepareRaster(targetLine)
    }
    val env =
        BandRenderEnv(
            playback = playback,
            resources = resources,
            currentTimeProvider = currentTime,
            color = activeColor,
            paints = paints,
            debug = showDebugRectangles,
            rightAligned = prepared.rightAligned,
            lineWidth = prepared.width,
            binding = prepared.binding,
            alignedness = alignedness,
        )
    val accompanimentAlpha =
        if (prepared.source is KaraokeLine.AccompanimentKaraokeLine) 0.6f else 1f
    // Prepared geometry already resolves reading direction into a physical edge.
    val alignment = if (prepared.rightAligned) AbsoluteAlignment.Right else AbsoluteAlignment.Left
    LyricsReveal(
        visible = playback.line(prepared).visible.value,
        animateInitial = prepared.source is KaraokeLine.AccompanimentKaraokeLine,
        origin =
            if (prepared.source is KaraokeLine.AccompanimentKaraokeLine)
                androidx.compose.ui.graphics.TransformOrigin(
                    if (prepared.rightAligned) 1f else 0f,
                    if (prepared.revealFromBottom) 1f else 0f,
                )
            else androidx.compose.ui.graphics.TransformOrigin.Center,
    ) {
        Column(
            modifier
                .graphicsLayer { alpha = accompanimentAlpha }
                .fillMaxWidth()
                .padding(
                    vertical = verticalPadding,
                    horizontal =
                        if (prepared.source is KaraokeLine.AccompanimentKaraokeLine) 0.dp else 16.dp,
                ),
            horizontalAlignment = alignment,
        ) {
            for (line in prepared.before) PreparedLineText(
                line,
                playback = playback,
                resources = resources,
                currentTimeProvider = currentTimeProvider,
                showTranslation = showTranslation,
                showPhonetic = showPhonetic,
                showDebugRectangles = showDebugRectangles,
            )
            // Separate draw scopes mean a ticking band cannot invalidate its static neighbours.
            Column(
                Modifier.fillMaxWidth(),
                horizontalAlignment = alignment,
            ) {
                val alignedRows = prepared.rows
                val compactRows = compact?.rows.orEmpty()
                val bandCount = maxOf(alignedRows.size, compactRows.size)
                for (band in 0 until bandCount) {
                    val alignedRow = alignedRows.getOrNull(band)
                    val compactRow = compactRows.getOrNull(band)
                    val alignedLayers = alignedRaster.value?.rows?.getOrNull(band)
                    val compactLayers = compactRaster?.value?.rows?.getOrNull(band)
                    val followingGap = alignedRows.getOrNull(band + 1)?.phoneticSpacingBefore ?: 0f
                    when {
                        // No compact geometry: captions own no width, so only they collapse.
                        compact == null && alignedRow != null -> {
                            BandText(
                                env,
                                alignedRow,
                                null,
                                alignedLayers,
                                null,
                                alignedRow.height - alignedRow.phoneticHeight,
                                showAligned = true,
                                showCompact = false,
                                blending = false,
                            )
                            BandCaption(
                                env,
                                alignedRow,
                                alignedLayers,
                                showPhonetic,
                                followingGap,
                                alignment,
                            )
                        }
                        // Both geometries keep this band: units that stay slide, units that move
                        // between bands are painted by both copies with complementary reveal.
                        alignedRow != null && compactRow != null -> {
                            val alignedHeight = alignedRow.height - alignedRow.phoneticHeight
                            val compactHeight = compactRow.height - compactRow.phoneticHeight
                            BandText(
                                env,
                                alignedRow,
                                compactRow,
                                alignedLayers,
                                compactLayers,
                                if (blending) maxOf(alignedHeight, compactHeight)
                                else if (currentAligned) alignedHeight else compactHeight,
                                showAligned = currentAligned || blending,
                                showCompact = !currentAligned || blending,
                                blending = blending,
                            )
                            BandCaption(
                                env,
                                alignedRow,
                                alignedLayers,
                                showPhonetic,
                                if (!blending && !currentAligned) 0f else followingGap,
                                alignment,
                            )
                        }
                        // A band only the aligned geometry has collapses with the caption spring.
                        alignedRow != null ->
                            LyricsRevealProgress(progress = alignedness) {
                                Column(horizontalAlignment = alignment) {
                                    BandText(
                                        env,
                                        alignedRow,
                                        null,
                                        alignedLayers,
                                        null,
                                        alignedRow.height - alignedRow.phoneticHeight,
                                        showAligned = true,
                                        showCompact = false,
                                        blending = false,
                                    )
                                    BandCaption(
                                        env,
                                        alignedRow,
                                        alignedLayers,
                                        showPhonetic,
                                        followingGap,
                                        alignment,
                                    )
                                }
                            }
                        // A band only the compact geometry has grows while alignment returns.
                        compactRow != null ->
                            LyricsRevealProgress(progress = { 1f - alignedness() }) {
                                Column(horizontalAlignment = alignment) {
                                    BandText(
                                        env,
                                        null,
                                        compactRow,
                                        null,
                                        compactLayers,
                                        compactRow.height - compactRow.phoneticHeight,
                                        showAligned = false,
                                        showCompact = true,
                                        blending = false,
                                    )
                                }
                            }
                    }
                }
            }
            if (prepared.translation != null || prepared.phonetic != null) {
                val captionGap =
                    animateFloatAsState(
                        if (
                            (showTranslation && prepared.translation != null) ||
                                (showPhonetic && prepared.phonetic != null)
                        )
                            8f
                        else 0f,
                        LyricsRevealSpring,
                        label = "captionGap",
                    )
                Spacer(
                    Modifier.layout { _, constraints ->
                        layout(
                            constraints.minWidth,
                            constraints.constrainHeight(captionGap.value.dp.roundToPx()),
                        ) {}
                    }
                )
            }
            prepared.translation?.let { layout ->
                LyricsReveal(showTranslation, keepContent = true) {
                    Canvas(
                        Modifier.size(
                            with(density) { layout.size.width.toDp() },
                            with(density) { layout.size.height.toDp() },
                        )
                    ) {
                        drawText(
                            layout,
                            activeColor.copy(alpha = activeColor.alpha * FocusedRowUnlitAlpha),
                        )
                    }
                }
            }
            prepared.phonetic?.let { layout ->
                LyricsReveal(showPhonetic, keepContent = true) {
                    Canvas(
                        Modifier.size(
                            with(density) { layout.size.width.toDp() },
                            with(density) { layout.size.height.toDp() },
                        )
                    ) {
                        drawText(layout, paints.phoneticColor)
                    }
                }
            }
            for (line in prepared.after) PreparedLineText(
                line,
                playback = playback,
                resources = resources,
                currentTimeProvider = currentTimeProvider,
                showTranslation = showTranslation,
                showPhonetic = showPhonetic,
                showDebugRectangles = showDebugRectangles,
            )
        }
    }
}

/** One band's text layer; blended bands paint both geometries inside the same box. */
@Composable
private fun BandText(
    env: BandRenderEnv,
    alignedRow: PreparedRow?,
    compactRow: PreparedRow?,
    alignedLayers: PreparedRowLayers?,
    compactLayers: PreparedRowLayers?,
    height: Float,
    showAligned: Boolean,
    showCompact: Boolean,
    blending: Boolean,
) {
    val density = LocalDensity.current
    Box(
        Modifier.fillMaxWidth()
            .height(with(density) { height.toDp() })
            .drawWithCache {
                val alignedGlows =
                    alignedLayers?.takeIf { showAligned && it.hasGlow }?.let {
                        traceLyrics("Lyrics.layerCache") { RowGlowLayers(this, it) }
                    }
                val compactGlows =
                    compactLayers?.takeIf { showCompact && it.hasGlow }?.let {
                        traceLyrics("Lyrics.layerCache") { RowGlowLayers(this, it) }
                    }
                onDrawBehind {
                    val reveal = env.alignedness()
                    if (showAligned && alignedRow != null && alignedLayers != null) {
                        translate(top = -alignedRow.top) {
                            drawPreparedRow(
                                alignedRow,
                                rowTime(env, alignedRow),
                                env.resources.row(alignedRow),
                                env.color,
                                env.paints,
                                env.debug,
                                alignedLayers,
                                alignedGlows,
                                drawPhonetics = false,
                                blend = env.blend(blending, reveal, aligned = true),
                            )
                        }
                    }
                    if (showCompact && compactRow != null && compactLayers != null) {
                        translate(top = -compactRow.top) {
                            drawPreparedRow(
                                compactRow,
                                rowTime(env, compactRow),
                                env.resources.row(compactRow),
                                env.color,
                                env.paints,
                                env.debug,
                                compactLayers,
                                compactGlows,
                                drawPhonetics = false,
                                blend = env.blend(blending, reveal, aligned = false),
                            )
                        }
                    }
                }
            }
    )
}

/** Pronunciation captions of one aligned row, anchored below the text it belongs to. */
@Composable
private fun BandCaption(
    env: BandRenderEnv,
    row: PreparedRow,
    layers: PreparedRowLayers?,
    showPhonetic: Boolean,
    followingGap: Float,
    alignment: androidx.compose.ui.Alignment.Horizontal,
) {
    if (row.phoneticHeight <= 0f) return
    val density = LocalDensity.current
    val textHeight = row.height - row.phoneticHeight
    val rowWidth = row.width
    LyricsReveal(showPhonetic, keepContent = true) {
        Column(horizontalAlignment = alignment) {
            Box(
                Modifier.size(
                    with(density) { rowWidth.toDp() },
                    with(density) { row.phoneticHeight.toDp() },
                ).drawWithCache {
                    // Layout rounds caption width to pixels; compensate here
                    // so end-aligned pronunciation keeps the original x origin.
                    val rowLeft = if (env.rightAligned) env.lineWidth - size.width else 0f
                    onDrawBehind {
                        if (layers == null) return@onDrawBehind
                        translate(left = -rowLeft, top = -row.top - textHeight) {
                            drawPreparedRow(
                                row,
                                rowTime(env, row),
                                env.resources.row(row),
                                env.color,
                                env.paints,
                                env.debug,
                                layers,
                                null,
                                drawOriginal = false,
                            )
                        }
                    }
                }
            )
            if (followingGap > 0f) Spacer(Modifier.height(with(density) { followingGap.toDp() }))
        }
    }
}

/** Reads the clock only for rows the timeline marked as animating. */
private fun rowTime(env: BandRenderEnv, row: PreparedRow): Int {
    val state = env.playback.row(row)
    if (!state.isAnimating.value) return state.time.intValue
    val now = env.currentTimeProvider()
    if (lyricsTraceEnabled()) {
        setLyricsTraceCounter("Lyrics.drawPositionMs", now.toLong())
        setLyricsTraceCounter("Lyrics.timelineLagMs", now.toLong() - state.time.intValue)
    }
    return now
}

private fun BandRenderEnv.blend(blending: Boolean, reveal: Float, aligned: Boolean): UnitBlend? =
    if (!blending || binding == null) null else UnitBlend(binding, reveal, aligned)
