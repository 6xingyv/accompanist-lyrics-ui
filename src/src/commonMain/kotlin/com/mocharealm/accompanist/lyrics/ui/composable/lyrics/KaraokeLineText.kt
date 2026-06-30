package com.mocharealm.accompanist.lyrics.ui.composable.lyrics

import androidx.compose.animation.core.CubicBezierEasing
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material3.LocalTextStyle
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.BlendMode
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Shadow
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.drawscope.drawIntoCanvas
import androidx.compose.ui.graphics.drawscope.scale
import androidx.compose.ui.graphics.drawscope.withTransform
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalFontFamilyResolver
import androidx.compose.ui.text.TextMeasurer
import androidx.compose.ui.text.TextStyle

import androidx.compose.ui.text.rememberTextMeasurer
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextDirection
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import com.mocharealm.accompanist.lyrics.core.model.karaoke.KaraokeAlignment
import com.mocharealm.accompanist.lyrics.core.model.karaoke.KaraokeLine
import com.mocharealm.accompanist.lyrics.text.NativeFontConfig
import com.mocharealm.accompanist.lyrics.text.NativeFontSource
import com.mocharealm.accompanist.lyrics.text.NativeTextEngine
import com.mocharealm.accompanist.lyrics.text.SdfAtlasManager
import com.mocharealm.accompanist.lyrics.text.drainPendingUploads
import com.mocharealm.accompanist.lyrics.text.rememberSdfAtlasManager
import com.mocharealm.accompanist.lyrics.ui.utils.LayerPaint
import com.mocharealm.accompanist.lyrics.ui.utils.easing.Bounce
import com.mocharealm.accompanist.lyrics.ui.utils.easing.DipAndRise
import com.mocharealm.accompanist.lyrics.ui.utils.easing.Swell
import com.mocharealm.accompanist.lyrics.ui.utils.isPunctuation
import com.mocharealm.accompanist.lyrics.ui.utils.isRtl
import org.jetbrains.compose.resources.FontResource
import kotlin.math.roundToInt

/**
 * Creates a horizontal gradient brush that represents the karaoke progress.
 * The gradient moves from inactive color to active color based on the current time.
 *
 * @param lineLayout The layout information for syllables in the line.
 * @param currentTimeMs The current playback time in milliseconds.
 * @param isRtl Whether the layout direction is Right-to-Left.
 */
private fun createLineGradientBrush(
    lineLayout: List<SyllableLayout>,
    currentTimeMs: Int,
    isRtl: Boolean
): Brush {
    val activeColor = Color.White
    val inactiveColor = Color.White.copy(alpha = 0.2f)
    val minFadeWidth = 100f

    if (lineLayout.isEmpty()) {
        return Brush.horizontalGradient(colors = listOf(inactiveColor, inactiveColor))
    }

    val totalMinX = lineLayout.minOf { it.position.x }
    val totalMaxX = lineLayout.maxOf { it.position.x + it.width }
    val totalWidth = totalMaxX - totalMinX

    if (totalWidth <= 0f) {
        val isFinished = currentTimeMs >= lineLayout.last().syllable.end
        val color = if (isFinished) activeColor else inactiveColor
        return Brush.horizontalGradient(listOf(color, color))
    }

    val firstSyllableStart = lineLayout.first().syllable.start
    val lastSyllableEnd = lineLayout.last().syllable.end

    val lineProgress = run {
        if (currentTimeMs <= firstSyllableStart) return Brush.horizontalGradient(
            listOf(inactiveColor, inactiveColor)
        )
        if (currentTimeMs >= lastSyllableEnd) return Brush.horizontalGradient(
            listOf(activeColor, activeColor)
        )

        val activeSyllableLayout = lineLayout.find {
            currentTimeMs in it.syllable.start until it.syllable.end
        }

        val currentPixelPosition = when {
            activeSyllableLayout != null -> {
                val syllableProgress = activeSyllableLayout.syllable.progress(currentTimeMs)
                if (isRtl) {
                    activeSyllableLayout.position.x + activeSyllableLayout.width * (1f - syllableProgress)
                } else {
                    activeSyllableLayout.position.x + activeSyllableLayout.width * syllableProgress
                }
            }
            else -> {
                val lastFinished = lineLayout.lastOrNull { currentTimeMs >= it.syllable.end }
                if (isRtl) {
                    lastFinished?.position?.x ?: totalMaxX
                } else {
                    lastFinished?.let { it.position.x + it.width } ?: totalMinX
                }
            }
        }
        ((currentPixelPosition - totalMinX) / totalWidth).coerceIn(0f, 1f)
    }

    val fadeRange = (minFadeWidth / totalWidth).coerceAtMost(1f)
    val fadeCenterStart = -fadeRange / 2f
    val fadeCenterEnd = 1f + fadeRange / 2f
    val fadeCenter = fadeCenterStart + (fadeCenterEnd - fadeCenterStart) * lineProgress
    val fadeStart = fadeCenter - fadeRange / 2f
    val fadeEnd = fadeCenter + fadeRange / 2f

    val colorStops = if (isRtl) {
        arrayOf(
            0.0f to inactiveColor,
            fadeStart.coerceIn(0f, 1f) to inactiveColor,
            fadeEnd.coerceIn(0f, 1f) to activeColor,
            1.0f to activeColor
        )
    } else {
        arrayOf(
            0.0f to activeColor,
            fadeStart.coerceIn(0f, 1f) to activeColor,
            fadeEnd.coerceIn(0f, 1f) to inactiveColor,
            1.0f to inactiveColor
        )
    }

    return Brush.horizontalGradient(
        colorStops = colorStops,
        startX = totalMinX,
        endX = totalMaxX
    )
}

/**
 * Draws a multi-row lyrics line into the canvas.
 * Handles row wrapping, padding, and applying the karaoke progress gradient.
 *
 * @param lineLayouts The pre-calculated layout of syllables, organized by rows.
 * @param currentTimeMs The current playback time in milliseconds.
 * @param color The base text color.
 * @param blendMode The blend mode to use for drawing.
 * @param isRtl Whether the layout direction is Right-to-Left.
 * @param showDebugRectangles Whether to draw debug outlines around glyphs.
 * @param atlasManager The SDF atlas manager for rendering text.
 */
fun DrawScope.drawLyricsLine(
    lineLayouts: List<List<SyllableLayout>>,
    currentTimeMs: Int,
    color: Color,
    blendMode: BlendMode,
    isRtl: Boolean,
    showDebugRectangles: Boolean = false,
    atlasManager: SdfAtlasManager? = null
) {
    lineLayouts.forEach { rowLayouts ->
        if (rowLayouts.isEmpty()) return@forEach

        val lastSyllableEnd = rowLayouts.last().syllable.end

        if (currentTimeMs >= lastSyllableEnd) {
            drawRowText(rowLayouts, color, blendMode, showDebugRectangles, currentTimeMs, atlasManager)
            return@forEach
        }

        val minX = rowLayouts.minOf { it.position.x }
        val maxX = rowLayouts.maxOf { it.position.x + it.width }
        val minY = rowLayouts.minOf { it.position.y }
        val totalHeight = rowLayouts.maxOf { it.layoutResult.size.height }.toFloat()

        val verticalPadding = (totalHeight * 0.1).dp.toPx()
        val horizontalPadding = ((maxX - minX) * 0.2).dp.toPx()

        drawIntoCanvas { canvas ->
            val layerBounds = Rect(
                left = minX - horizontalPadding,
                top = minY - verticalPadding,
                right = maxX + horizontalPadding,
                bottom = minY + totalHeight + verticalPadding
            )
            canvas.saveLayer(layerBounds, LayerPaint)

            drawRowText(rowLayouts, color, blendMode, showDebugRectangles, currentTimeMs, atlasManager)

            val progressBrush = createLineGradientBrush(rowLayouts, currentTimeMs, isRtl)
            drawRect(
                brush = progressBrush,
                topLeft = layerBounds.topLeft,
                size = layerBounds.size,
                blendMode = BlendMode.DstIn
            )
            canvas.restore()
        }
    }
}

/**
 * Draws text for a single row, handling word and character animations.
 *
 * @param rowLayouts The layouts for syllables in this row.
 * @param drawColor The color to draw the text with.
 * @param blendMode The blend mode to use.
 * @param showDebugRectangles Whether to show debug bounds.
 * @param currentTimeMs Current playback time.
 * @param atlasManager SDF atlas manager for rendering.
 */
private fun DrawScope.drawRowText(
    rowLayouts: List<SyllableLayout>,
    drawColor: Color,
    blendMode: BlendMode,
    showDebugRectangles: Boolean,
    currentTimeMs: Int,
    atlasManager: SdfAtlasManager? = null
) {
    rowLayouts.forEachIndexed { index, syllableLayout ->
        val wordAnimInfo = syllableLayout.wordAnimInfo

        if (wordAnimInfo != null) {
            val fastCharAnimationThresholdMs = 200f
            val awesomeDuration = wordAnimInfo.wordDuration * 0.8f

            val charLayouts = syllableLayout.charLayouts ?: emptyList()
            val charBounds = syllableLayout.charOriginalBounds ?: emptyList()

            syllableLayout.syllable.content.forEachIndexed { charIndex, _ ->
                val singleCharLayoutResult = charLayouts.getOrNull(charIndex) ?: return@forEachIndexed
                val charBox = charBounds.getOrNull(charIndex) ?: return@forEachIndexed

                val absoluteCharIndex = syllableLayout.charOffsetInWord + charIndex
                val numCharsInWord = wordAnimInfo.wordContent.length
                val earliestStartTime = wordAnimInfo.wordStartTime
                val latestStartTime = wordAnimInfo.wordEndTime - awesomeDuration

                val charRatio = if (numCharsInWord > 1) absoluteCharIndex.toFloat() / (numCharsInWord - 1) else 0.5f
                val awesomeStartTime = (earliestStartTime + (latestStartTime - earliestStartTime) * charRatio).toLong()
                val awesomeProgress = ((currentTimeMs - awesomeStartTime).toFloat() / awesomeDuration).coerceIn(0f, 1f)

                val floatOffset = 4f * DipAndRise(
                    dip = ((0.5 * (wordAnimInfo.wordDuration - fastCharAnimationThresholdMs * numCharsInWord) / 1000)).coerceIn(0.0, 0.5)
                ).transform(1.0f - awesomeProgress)
                val scale = 1f + Swell(
                    (0.1 * (wordAnimInfo.wordDuration - fastCharAnimationThresholdMs * numCharsInWord) / 1000).coerceIn(0.0, 0.1)
                ).transform(awesomeProgress)

                val centeredOffsetX = (charBox.width - singleCharLayoutResult.size.width) / 2f
                val xPos = syllableLayout.position.x + charBox.left + centeredOffsetX
                val yPos = syllableLayout.position.y + charBox.top + floatOffset

                withTransform({
                    scale(scale = scale, pivot = syllableLayout.wordPivot)
                    translate(left = xPos, top = yPos)
                }) {
                    // Draw glyph from atlas with shadow
                    val glyphSize = Size(
                        singleCharLayoutResult.size.width.toFloat(),
                        singleCharLayoutResult.size.height.toFloat()
                    )
                    
                    // Create shadow based on animation progress (same as original implementation)
                    val blurRadius = 10f * Bounce.transform(awesomeProgress)
                    val shadow = if (blurRadius > 0f) {
                        Shadow(
                            color = drawColor.copy(alpha = 0.4f),
                            offset = Offset.Zero,
                            blurRadius = blurRadius
                        )
                    } else null
                    
                    drawGlyphsFromLayout(singleCharLayoutResult, Offset.Zero, drawColor, atlasManager, shadow)
                    
                    if (showDebugRectangles) {
                        drawRect(
                            color = Color.Red,
                            topLeft = Offset.Zero,
                            size = glyphSize,
                            style = Stroke(1f)
                        )
                    }
                }
            }
        } else {
            val driverLayout = if (syllableLayout.syllable.content.trim().isPunctuation()) {
                var searchIndex = index - 1
                while (searchIndex >= 0) {
                    val candidate = rowLayouts[searchIndex]
                    if (!candidate.syllable.content.trim().isPunctuation()) {
                        break
                    }
                    searchIndex--
                }
                if (searchIndex < 0) syllableLayout else rowLayouts[searchIndex]
            } else {
                syllableLayout
            }

            val animationFixedDuration = (driverLayout.floatEndingTime - driverLayout.syllable.start).toFloat().coerceAtLeast(1f)
            val timeSinceStart = currentTimeMs - driverLayout.syllable.start
            val animationProgress = (timeSinceStart / animationFixedDuration).coerceIn(0f, 1f)

            val maxOffsetY = 4f
            val floatCurveValue = CubicBezierEasing(0.0f, 0.0f, 0.2f, 1.0f).transform(1f - animationProgress)
            val floatOffset = maxOffsetY * floatCurveValue

            val finalPosition = syllableLayout.position.copy(
                y = syllableLayout.position.y + floatOffset
            )

            withTransform({
                translate(left = finalPosition.x, top = finalPosition.y)
            }) {
                // Draw glyphs from atlas
                val glyphSize = Size(
                    syllableLayout.layoutResult.size.width.toFloat(),
                    syllableLayout.layoutResult.size.height.toFloat()
                )
                drawGlyphsFromLayout(syllableLayout.layoutResult, Offset.Zero, drawColor, atlasManager)
                
                if (showDebugRectangles) {
                    drawRect(
                        color = Color.Green,
                        topLeft = Offset.Zero,
                        size = glyphSize,
                        style = Stroke(2f)
                    )
                }
            }
        }
    }
}

/**
 * Draws all glyphs from a NativeLayoutResult using the SdfAtlasManager.
 * Each glyph is drawn at its position using atlas coordinates.
 * Uses glyph bearings (offsets) for proper baseline alignment.
 */
/**
 * Draws all glyphs from a NativeLayoutResult using the SdfAtlasManager.
 * Each glyph is drawn at its position using atlas coordinates.
 * Uses glyph bearings (offsets) for proper baseline alignment.
 *
 * @param layout The native layout result containing glyph positions and sizing.
 * @param baseOffset The offset to apply to all glyphs (e.g., line position).
 * @param color The color to tint the glyphs.
 * @param atlasManager The manager containing the font texture atlas.
 * @param shadow Optional shadow to apply to the glyphs.
 */
private fun DrawScope.drawGlyphsFromLayout(
    layout: NativeLayoutResult,
    baseOffset: Offset,
    color: Color,
    atlasManager: SdfAtlasManager?,
    shadow: Shadow? = null
) {
    if (layout.glyph_count == 0) return
    
    // positions array is x, y interleaved (relative to baseline, y=0 means on baseline)
    // atlas_rects array is x, y, w, h interleaved (atlas coordinates)
    // glyph_offsets array is x, y interleaved (bearing from glyph origin to bitmap top-left)
    for (i in 0 until layout.glyph_count) {
        val posIndex = i * 2
        val rectIndex = i * 4
        val offsetIndex = i * 2
        
        if (posIndex + 1 >= layout.positions.size) break
        if (rectIndex + 3 >= layout.atlas_rects.size) break
        
        // Glyph position relative to baseline (from shaping)
        val glyphX = layout.positions[posIndex]
        val glyphY = layout.positions[posIndex + 1]
        
        // Glyph bearing offsets (from glyph origin to bitmap top-left)
        val bearingX = if (offsetIndex + 1 < layout.glyph_offsets.size) layout.glyph_offsets[offsetIndex] else 0f
        val bearingY = if (offsetIndex + 1 < layout.glyph_offsets.size) layout.glyph_offsets[offsetIndex + 1] else 0f
        
        val atlasX = layout.atlas_rects[rectIndex]
        val atlasY = layout.atlas_rects[rectIndex + 1]
        val atlasW = layout.atlas_rects[rectIndex + 2]
        val atlasH = layout.atlas_rects[rectIndex + 3]
        
        if (atlasW <= 0f || atlasH <= 0f) continue
        
        val atlasRect = Rect(atlasX, atlasY, atlasX + atlasW, atlasY + atlasH)
        
        // Calculate final position:
        // - baseOffset.y is the top of the line
        // - layout.ascent moves us down to the baseline
        // - glyphY is vertical offset from baseline (usually 0 for horizontal text)
        // 
        // In fontdue, ymin is the offset from baseline to the BOTTOM edge of the bitmap.
        // Positive ymin means the bottom is above the baseline.
        // So the TOP of the bitmap is at: baseline - (ymin + height)
        // But we have SDF padding included in bearingY, so:
        //   bitmap top = baseline - bearingY - atlasH
        // In screen coords (y down): subtract moves up
        val destX = baseOffset.x + glyphX + bearingX
        val destY = baseOffset.y + layout.ascent + glyphY - bearingY - atlasH
        
        val destOffset = Offset(destX, destY)
        val destSize = Size(atlasW, atlasH)
        
        if (atlasManager != null && atlasManager.isReady()) {
            with(atlasManager) {
                drawGlyph(atlasRect, destOffset, destSize, color, shadow)
            }
        } else {
            // Fallback: draw colored rectangle if atlas is not ready
            drawRect(
                color = color,
                topLeft = destOffset,
                size = destSize
            )
        }
    }
}

/**
 * Renders a single karaoke line, capable of handling multi-row wrapping.
 *
 * This composable pre-calculates the text layout using [NativeTextEngine] and then
 * renders the frames using an efficient Canvas drawing strategy. It handles:
 * - Text measurement and line breaking
 * - Syllable and character-level animations (bounce, rise, swell)
 * - Karaoke fill gradient application
 *
 * @param line The karaoke line data.
 * @param currentTimeProvider Provider for the current playback time.
 * @param modifier Modifier for the layout.
 * @param normalLineTextStyle Style for normal lines.
 * @param accompanimentLineTextStyle Style for accompaniment lines.
 * @param activeColor Color for the active (sung) portion of text.
 * @param blendMode Blend mode for drawing.
 * @param showDebugRectangles Debug flag for layout bounds.
 * @param precalculatedLayouts Optional pre-calculated layouts (optimization).
 * @param isDuoView Whether this line is part of a duet view.
 * @param textMeasurer Text measurer for layout (default provided).
 * @param fontResource Optional font resource for the native engine.
 * @param sharedNativeEngine Optional shared native engine instance.
 * @param sharedAtlasManager Optional shared atlas manager instance.
 */
@Composable
fun KaraokeLineText(
    line: KaraokeLine,
    currentTimeProvider: () -> Int,
    modifier: Modifier = Modifier,
    normalLineTextStyle: TextStyle = LocalTextStyle.current,
    accompanimentLineTextStyle: TextStyle = LocalTextStyle.current,
    activeColor: Color = Color.White,
    blendMode: BlendMode = BlendMode.SrcOver,
    showDebugRectangles: Boolean = false,
    precalculatedLayouts: List<SyllableLayout>? = null,
    isDuoView: Boolean = false,
    textMeasurer: TextMeasurer = rememberTextMeasurer(),
    fontResource: FontResource? = null, // CMP font resource for NativeTextEngine
    sharedNativeEngine: NativeTextEngine? = null, // Shared engine for atlas reuse
    sharedAtlasManager: SdfAtlasManager? = null // Shared atlas manager
) {
    val isLineRtl = remember(line.syllables) { line.syllables.any { it.content.isRtl() } }

    val isRightAligned = remember(line.alignment, isLineRtl) {
        when (line.alignment) {
            KaraokeAlignment.Start, KaraokeAlignment.Unspecified -> isLineRtl
            KaraokeAlignment.End -> !isLineRtl
        }
    }

    val translationTextAlign = remember(isRightAligned) {
        if (isRightAligned) TextAlign.End else TextAlign.Start
    }

    val columnHorizontalAlignment = remember(isRightAligned) {
        if (isRightAligned) Alignment.End else Alignment.Start
    }

    Column(
        modifier = modifier
            .fillMaxWidth()
            .padding(vertical = 8.dp, horizontal = 16.dp),
        verticalArrangement = Arrangement.spacedBy(2.dp),
        horizontalAlignment = columnHorizontalAlignment
    ) {
        BoxWithConstraints {
            val density = LocalDensity.current
            val fontFamilyResolver = LocalFontFamilyResolver.current
            val platformContext = getPlatformContext()
            val fontResourceBytes = rememberFontResourceBytes(fontResource)
            val availableWidthPx = with(density) { maxWidth.toPx() }
            val isAccompanimentLine = line is KaraokeLine.AccompanimentKaraokeLine

            val textStyle = remember(isAccompanimentLine, normalLineTextStyle, accompanimentLineTextStyle) {
                val baseStyle = if (isAccompanimentLine) accompanimentLineTextStyle else normalLineTextStyle
                baseStyle.copy(textDirection = TextDirection.Content)
            }

            val spaceWidth = remember(textMeasurer, textStyle) {
                textMeasurer.measure(" ", textStyle).size.width.toFloat()
            }

            val processedSyllables = remember(line.syllables, line.alignment) {
                if (line.alignment == KaraokeAlignment.End) {
                    line.syllables.dropLastWhile { it.content.isBlank() }
                } else {
                    line.syllables
                }
            }

            val fontConfig = remember(textStyle.fontFamily, platformContext, fontResourceBytes) {
                NativeFontConfig(
                    primary = fontResourceBytes?.let { NativeFontSource(bytes = it) }
                        ?: getFontSource(textStyle.fontFamily, platformContext),
                    fallbacks = getSystemFallbackFontSources(platformContext)
                )
            }
            val fontConfigKey = remember(fontConfig) { fontConfig.stableKey() }

            val nativeEngine = sharedNativeEngine ?: remember(fontConfigKey) {
                NativeTextEngine(2048, 2048).apply {
                    configureFonts(fontConfig)
                }
            }
            
            // Create SdfAtlasManager with same dimensions as native engine (use shared if provided)
            val atlasManager = sharedAtlasManager ?: rememberSdfAtlasManager(2048, 2048)

            LaunchedEffect(sharedNativeEngine, fontConfigKey) {
                if (sharedNativeEngine == null) {
                    atlasManager.clear()
                }
            }

            DisposableEffect(nativeEngine, sharedNativeEngine) {
                onDispose {
                    if (sharedNativeEngine == null) {
                        nativeEngine.close()
                    }
                }
            }

            val initialLayouts by remember(
                precalculatedLayouts,
                processedSyllables,
                textStyle,
                spaceWidth,
                density.density,
                density.fontScale,
                fontConfigKey,
                nativeEngine,
                nativeEngine.generation
            ) {
                derivedStateOf {
                    precalculatedLayouts ?: measureSyllablesAndDetermineAnimation(
                        syllables = processedSyllables,
                        textMeasurer = textMeasurer,
                        style = textStyle,
                        isAccompanimentLine = isAccompanimentLine,
                        spaceWidth = spaceWidth,
                        fontFamilyResolver = fontFamilyResolver,
                        density = density,
                        nativeEngine = nativeEngine,
                        platformContext = platformContext
                    )
                }
            }

            val wrappedLines by remember(
                initialLayouts,
                availableWidthPx,
                textStyle,
                density.density,
                density.fontScale,
                nativeEngine,
                nativeEngine.generation
            ) {
                derivedStateOf {
                    calculateBalancedLines(
                        syllableLayouts = initialLayouts,
                        availableWidthPx = availableWidthPx,
                        nativeEngine = nativeEngine,
                        style = textStyle,
                        density = density
                    )
                }
            }

            val lineHeight = remember(textStyle) {
                // Use native engine for 'M' measurement too?
                // Or keep TextMeasurer for simple generic metrics? 
                // Let's keep TextMeasurer for line height for now to avoid JSON parsing overhead for simple height.
                textMeasurer.measure("M", textStyle).size.height.toFloat()
            }

            val finalLineLayouts = remember(wrappedLines, availableWidthPx, lineHeight,
                isLineRtl, isRightAligned) {
                calculateStaticLineLayout(
                    wrappedLines = wrappedLines,
                    isLineRightAligned = isRightAligned,
                    canvasWidth = availableWidthPx,
                    lineHeight = lineHeight,
                    isRtl = isLineRtl
                )
            }

            val totalHeight = remember(wrappedLines, lineHeight) {
                lineHeight * wrappedLines.size
            }
            
            LaunchedEffect(initialLayouts, wrappedLines, nativeEngine, nativeEngine.generation) {
                val uploads = nativeEngine.drainPendingUploads()
                if (uploads.isNotEmpty()) {
                    atlasManager.updateAtlas(uploads)
                }
            }

            Canvas(modifier = Modifier.size(maxWidth, (totalHeight.roundToInt() + 8).toDp())) {
                val time = currentTimeProvider()
                drawLyricsLine(
                    lineLayouts = finalLineLayouts,
                    currentTimeMs = time,
                    color = activeColor,
                    blendMode = blendMode,
                    isRtl = isLineRtl,
                    showDebugRectangles = showDebugRectangles,
                    atlasManager = atlasManager
                )
            }
        }

        line.translation?.let { translation ->
            Text(
                text = translation,
                color = activeColor.copy(0.4f),
                modifier = Modifier.graphicsLayer {
                    this.blendMode = blendMode
                },
                textAlign = translationTextAlign
            )
        }
    }
}

@Composable
private fun Int.toDp(): Dp = with(LocalDensity.current) { this@toDp.toDp() }
