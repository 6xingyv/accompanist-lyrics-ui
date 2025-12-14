package com.mocharealm.accompanist.lyrics.ui.composable.lyrics

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.Crossfade
import androidx.compose.animation.ExperimentalSharedTransitionApi
import androidx.compose.animation.core.animateDpAsState
import androidx.compose.animation.core.tween
import androidx.compose.animation.expandVertically
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.scaleIn
import androidx.compose.animation.scaleOut
import androidx.compose.animation.shrinkVertically
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutVertically
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.gestures.animateScrollBy
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.LocalTextStyle
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.BlurredEdgeTreatment
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.blur
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawWithCache
import androidx.compose.ui.graphics.BlendMode
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.TransformOrigin
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalWindowInfo
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextMotion
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.em
import androidx.compose.ui.unit.sp
import com.mocharealm.accompanist.lyrics.core.model.ISyncedLine
import com.mocharealm.accompanist.lyrics.core.model.SyncedLyrics
import com.mocharealm.accompanist.lyrics.core.model.karaoke.KaraokeAlignment
import com.mocharealm.accompanist.lyrics.core.model.karaoke.KaraokeLine
import com.mocharealm.accompanist.lyrics.core.model.synced.SyncedLine
import com.mocharealm.accompanist.lyrics.ui.theme.SFPro
import kotlin.math.abs

@OptIn(ExperimentalSharedTransitionApi::class)
@Composable
fun KaraokeLyricsView(
    listState: LazyListState,
    lyrics: SyncedLyrics,
    currentPosition: Long,
    onLineClicked: (ISyncedLine) -> Unit,
    onLinePressed: (ISyncedLine) -> Unit,
    modifier: Modifier = Modifier,
    normalLineTextStyle: TextStyle = LocalTextStyle.current.copy(
        fontSize = 34.sp,
        fontWeight = FontWeight.Bold,
        fontFamily = SFPro(),
        textMotion = TextMotion.Animated,
    ),
    accompanimentLineTextStyle: TextStyle = LocalTextStyle.current.copy(
        fontSize = 20.sp,
        fontWeight = FontWeight.Bold,
        fontFamily = SFPro(),
        textMotion = TextMotion.Animated,
    ),
    textColor: Color = Color.White,
    verticalFadeMask: Modifier = Modifier.drawWithCache {
        onDrawWithContent {
            drawContent()
            drawRect(
                Brush.verticalGradient(
                    0f to Color.Transparent,
                    0.1f to Color.White,
                    0.5f to Color.White,
                    1f to Color.Transparent
                ),
                blendMode = BlendMode.DstIn
            )
        }
    },
    breathingDotsColor: Color = Color.White
) {
    val currentTimeMs by rememberUpdatedState(currentPosition.toInt())

    val rawFirstFocusedLineIndex = lyrics.getCurrentFirstHighlightLineIndexByTime(currentTimeMs)

    val finalFirstFocusedLineIndex by remember(currentTimeMs, lyrics.lines) {
        derivedStateOf {
            val line = lyrics.lines.getOrNull(rawFirstFocusedLineIndex) as? KaraokeLine
            if (line != null && line.isAccompaniment) {
                var newIndex = rawFirstFocusedLineIndex
                for (i in rawFirstFocusedLineIndex downTo 0) {
                    if (!(lyrics.lines[i] as KaraokeLine).isAccompaniment) {
                        newIndex = i
                        break
                    }
                }
                newIndex
            } else {
                rawFirstFocusedLineIndex
            }
        }
    }

    val isDuoView by remember {
        derivedStateOf {
            var hasStart = false
            var hasEnd = false

            if (lyrics.lines.isEmpty()) {
                return@derivedStateOf false
            }

            for (line in lyrics.lines) {
                if (line is KaraokeLine) {
                    when (line.alignment) {
                        KaraokeAlignment.Start -> hasStart = true
                        KaraokeAlignment.End -> hasEnd = true
                        else -> {}
                    }
                }
                if (hasStart && hasEnd) {
                    break
                }
            }
            hasStart && hasEnd
        }
    }
    val windowHeightPx= LocalWindowInfo.current.containerSize.height
    val windowHeight = with(LocalDensity.current) {
        windowHeightPx.toDp()
    }

    val baseScrollOffset = windowHeightPx * 0.1f

    var isScrollProgrammatically by remember { mutableStateOf(false) }

    LaunchedEffect(finalFirstFocusedLineIndex) {
        if (finalFirstFocusedLineIndex >= 0 &&
            finalFirstFocusedLineIndex < lyrics.lines.size &&
            !listState.isScrollInProgress
        ) {
            isScrollProgrammatically = true
            val items = listState.layoutInfo.visibleItemsInfo
            val targetItem = items.firstOrNull { it.index == finalFirstFocusedLineIndex }
            val scrollOffset =
                (targetItem?.offset?.minus(listState.layoutInfo.viewportStartOffset + baseScrollOffset))?.toFloat()
            try {
                if (scrollOffset != null) {
                    listState.animateScrollBy(scrollOffset, tween(600))
                } else {
                    listState.animateScrollToItem(finalFirstFocusedLineIndex)
                }
            } catch (_: Exception) {
            }
            isScrollProgrammatically = false
        }
    }
    val firstLine = lyrics.lines.firstOrNull() ?: SyncedLine("", null, 0, 0)
    val showDotInIntro = remember(firstLine, currentTimeMs) {
        (firstLine.start > 5000) && (currentTimeMs in 0 until firstLine.start)
    }

    val allFocusedLineIndex by rememberUpdatedState(
        lyrics.getCurrentAllHighlightLineIndicesByTime(
            currentTimeMs
        )
    )

    val accompanimentFocusedLines by remember(lyrics, currentTimeMs) {
        derivedStateOf {
            lyrics.lines.filter {
                it is KaraokeLine && it.isAccompaniment &&
                        (currentTimeMs in it.start..it.end || (abs(
                            currentTimeMs - it.start
                        )) < 600 || (abs(currentTimeMs - it.end)) < 600)
            }
        }
    }

    Crossfade(lyrics) { lyrics ->
        LazyColumn(
            state = listState,
            modifier = modifier
                .fillMaxSize()
                .then(verticalFadeMask),
            contentPadding = PaddingValues(vertical = 300.dp)
        ) {
            item(key = "intro-dots") {

                if (showDotInIntro) {
                    KaraokeBreathingDots(
                        alignment = (firstLine as? KaraokeLine)?.alignment
                            ?: KaraokeAlignment.Start,
                        startTimeMs = 0,
                        endTimeMs = firstLine.start,
                        currentTimeMs = currentTimeMs,
                    )
                }
            }
            itemsIndexed(
                items = lyrics.lines,
                key = { index, line -> "${line.start}-${line.end}-$index" }
            ) { index, line ->
                val isCurrentFocusLine by rememberUpdatedState(index in allFocusedLineIndex)
                val positionToFocusedLine = remember(index, allFocusedLineIndex) {
                    val isCurrentFocusLine = index in allFocusedLineIndex

                    val position = if (isCurrentFocusLine) {
                        0
                    } else if (allFocusedLineIndex.isNotEmpty() && index > allFocusedLineIndex.last()) {
                        index - allFocusedLineIndex.last()
                    } else if (allFocusedLineIndex.isNotEmpty() && index < allFocusedLineIndex.first()) {
                        allFocusedLineIndex.first() - index
                    } else {
                        2
                    }
                    position.toFloat()
                }
                when (line) {
                    is KaraokeLine -> {
                        if (!line.isAccompaniment) {
                            val blurRadius by animateDpAsState(
                                targetValue = if (listState.isScrollInProgress && !isScrollProgrammatically) {
                                    0.dp
                                } else {
                                    positionToFocusedLine.coerceAtMost(10f).dp
                                },
                                label = "blurAnimation"
                            )

                            val isLineDone by rememberUpdatedState(currentTimeMs >= line.end)

                            val lineTimeMs =
                                if (isCurrentFocusLine) currentTimeMs else if (isLineDone) line.end + 50 else 0
                            KaraokeLineText(
                                line = line,
                                onLineClicked = onLineClicked,
                                onLinePressed = onLinePressed,
                                currentTimeMs = lineTimeMs,
                                modifier = Modifier
                                    .fillMaxWidth(if (isDuoView) 0.85f else 1f)
                                    .blur(
                                        blurRadius,
                                        BlurredEdgeTreatment.Unbounded
                                    ),
                                normalLineTextStyle = normalLineTextStyle,
                                accompanimentLineTextStyle = accompanimentLineTextStyle,
                                activeColor = textColor
                            )
                        } else {
                            val isCurrentFocusLine by rememberUpdatedState(
                                accompanimentFocusedLines.contains(line)
                            )
                            val isLineDone by rememberUpdatedState(currentTimeMs - line.end >= 600)

                            val lineTimeMs =
                                if (isCurrentFocusLine) currentTimeMs else if (isLineDone) line.end + 600 else 0
                            AnimatedVisibility(
                                visible = isCurrentFocusLine,
                                enter = scaleIn(
                                    transformOrigin = TransformOrigin(
                                        if (line.alignment == KaraokeAlignment.Start) 0f else 1f,
                                        0f
                                    ),
                                    animationSpec = tween(600)
                                ) + fadeIn() + slideInVertically(
                                    animationSpec = tween(600)
                                ) + expandVertically(
                                    animationSpec = tween(600)
                                ),
                                exit = scaleOut(
                                    transformOrigin = TransformOrigin(
                                        if (line.alignment == KaraokeAlignment.Start) 0f else 1f,
                                        0f
                                    ),
                                    animationSpec = tween(600)
                                ) + fadeOut() + slideOutVertically(animationSpec = tween(600)) + shrinkVertically(
                                    animationSpec = tween(600)
                                ),
                            ) {
                                KaraokeLineText(
                                    line = line,
                                    onLineClicked = onLineClicked,
                                    onLinePressed = onLinePressed,
                                    currentTimeMs = lineTimeMs,
                                    Modifier
                                        .fillMaxWidth(if (isDuoView) 0.85f else 1f).alpha(0.8f),
                                    normalLineTextStyle = normalLineTextStyle,
                                    accompanimentLineTextStyle = accompanimentLineTextStyle,
                                    activeColor = textColor
                                )
                            }
                        }
                    }

                    is SyncedLine -> {
                        val animatedScale by animateFloatAsState(targetValue = if (isCurrentFocusLine) 1.05f else 1f, label = "scale")
                        val alphaAnimation by animateFloatAsState(
                            targetValue = if (isCurrentFocusLine) 1f else 0.4f,
                            label = "alpha"
                        )
                        val blurRadius by animateDpAsState(
                            targetValue = if (listState.isScrollInProgress && !isScrollProgrammatically) {
                                0.dp
                            } else {
                                positionToFocusedLine.coerceAtMost(10f).dp
                            },
                            label = "blurAnimation"
                        )


                        Box(
                            Modifier
                                .fillMaxWidth()
                                .clip(RoundedCornerShape(8.dp))
                                .blur(
                                    blurRadius,
                                    BlurredEdgeTreatment.Unbounded
                                )
                                .combinedClickable(onClick = { onLineClicked(line) }, onLongClick = { onLinePressed(line) })
                                .graphicsLayer {
                                    scaleX = animatedScale
                                    scaleY = animatedScale
                                    alpha = alphaAnimation
                                    transformOrigin = TransformOrigin(0f, 1f)
                                },
                        ) {
                            Column(
                                Modifier
                                    .align(Alignment.TopStart)
                                    .padding(vertical = 8.dp, horizontal = 16.dp)
                            ) {
                                //Text("Start: ${line.start} ms\nEnd:${line.end} ms", color = Color.White.copy(0.6f))
                                Text(
                                    text = line.content,
                                    style = normalLineTextStyle.copy(lineHeight = 1.2f.em),
                                    color = textColor
                                )
                                line.translation?.let {
                                    Text(it, color = textColor.copy(0.6f))
                                }
                            }
                        }
                    }
                }

                val nextLine = lyrics.lines.getOrNull(index + 1)
                val showDotInPause = remember(line, nextLine, currentTimeMs) {
                    nextLine != null &&
                            (nextLine.start - line.end > 5000) &&
                            (currentTimeMs in line.end..nextLine.start)
                }
                AnimatedVisibility(showDotInPause) {
                    KaraokeBreathingDots(
                        alignment = (nextLine as? KaraokeLine)?.alignment ?: KaraokeAlignment.Start,
                        startTimeMs = line.end,
                        endTimeMs = nextLine!!.start,
                        currentTimeMs = currentTimeMs,
                        breathingDotsColor = breathingDotsColor
                    )
                }
            }

            item("BottomSpacing") {
                Spacer(
                    modifier = Modifier
                        .fillMaxWidth()
                        .height(windowHeight)
                )
            }
        }
    }
}