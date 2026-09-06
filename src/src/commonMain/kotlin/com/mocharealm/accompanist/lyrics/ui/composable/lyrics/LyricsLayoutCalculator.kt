package com.mocharealm.accompanist.lyrics.ui.composable.lyrics

import androidx.compose.runtime.Stable
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.text.TextLayoutResult
import androidx.compose.ui.text.TextMeasurer
import androidx.compose.ui.text.TextStyle
import com.mocharealm.accompanist.lyrics.core.model.karaoke.KaraokeSyllable
import com.mocharealm.accompanist.lyrics.ui.utils.getGraphemeClusters
import com.mocharealm.accompanist.lyrics.ui.utils.isArabic
import com.mocharealm.accompanist.lyrics.ui.utils.isIndic
import com.mocharealm.accompanist.lyrics.ui.utils.isPunctuation
import com.mocharealm.accompanist.lyrics.ui.utils.isPureCjk
import kotlin.math.pow

@Stable
data class SyllableLayout(
    val syllable: KaraokeSyllable,
    val textLayoutResult: TextLayoutResult,
    val wordId: Int,
    val useAwesomeAnimation: Boolean,
    val width: Float = textLayoutResult.size.width.toFloat(),
    val position: Offset = Offset.Zero,
    val wordPivot: Offset = Offset.Zero,
    val wordAnimInfo: WordAnimationInfo? = null,
    val charOffsetInWord: Int = 0,
    val charLayouts: List<TextLayoutResult>? = null,
    val charOriginalBounds: List<Rect>? = null,
    val firstBaseline: Float = 0f,
    val phonetic: String? = null,
    val phoneticLayoutResult: TextLayoutResult? = null,
)

@Stable
data class WordAnimationInfo(
    val wordStartTime: Long,
    val wordEndTime: Long,
    val wordContent: String,
    val wordDuration: Long = wordEndTime - wordStartTime,
    val totalClusters: Int = wordContent.getGraphemeClusters().size
)

@Stable
data class WrappedLine(
    val syllables: List<SyllableLayout>, val totalWidth: Float
)

@Stable
data class RowRenderData(
    val rowLayouts: List<SyllableLayout>,
    val totalMinX: Float,
    val totalMaxX: Float,
    val totalWidth: Float,
    val firstSyllableStart: Int,
    val lastSyllableEnd: Int,
    val layerBounds: Rect
)

fun calculateRowRenderData(
    lineLayouts: List<List<SyllableLayout>>,
    showPhonetic: Boolean,
    density: Float,
    edgePaddingDp: Float = 8f
): List<RowRenderData> {
    return lineLayouts.mapNotNull { rowLayouts ->
        if (rowLayouts.isEmpty()) return@mapNotNull null

        val totalMinX = rowLayouts.minOf { it.position.x }
        val totalMaxX = rowLayouts.maxOf { it.position.x + it.width }
        val totalWidth = totalMaxX - totalMinX
        val minY = rowLayouts.minOf { it.position.y }
        val totalHeight = rowLayouts.maxOf { it.textLayoutResult.size.height }.toFloat()
        val maxPhoneticHeight = if (showPhonetic) {
            rowLayouts.maxOfOrNull { it.phoneticLayoutResult?.size?.height ?: 0 }?.toFloat() ?: 0f
        } else {
            0f
        }

        val verticalPadding = (totalHeight * 0.1f) * density
        val horizontalPadding = ((totalMaxX - totalMinX) * 0.2f) * density
        val edgePaddingPx = edgePaddingDp * density

        RowRenderData(
            rowLayouts = rowLayouts,
            totalMinX = totalMinX,
            totalMaxX = totalMaxX,
            totalWidth = totalWidth,
            firstSyllableStart = rowLayouts.first().syllable.start,
            lastSyllableEnd = rowLayouts.last().syllable.end,
            layerBounds = Rect(
                left = totalMinX - horizontalPadding,
                top = minY - verticalPadding - maxPhoneticHeight - edgePaddingPx,
                right = totalMaxX + horizontalPadding,
                bottom = minY + totalHeight + verticalPadding + edgePaddingPx
            )
        )
    }
}

fun String.shouldUseSimpleAnimation(
    enableSyllableBounce: Boolean = true,
    enableIndicSyllableBounce: Boolean = true
): Boolean {
    if (!enableSyllableBounce) return true
    val cleanedStr = this.filter { !it.isWhitespace() && !it.toString().isPunctuation() }
    if (cleanedStr.isEmpty()) return false
    if (!enableIndicSyllableBounce && cleanedStr.any { it.isIndic() }) return true
    return cleanedStr.isPureCjk() || cleanedStr.any { it.isArabic() }
}

fun groupIntoWords(syllables: List<KaraokeSyllable>): List<List<KaraokeSyllable>> {
    if (syllables.isEmpty()) return emptyList()
    val words = mutableListOf<List<KaraokeSyllable>>()
    var currentWord = mutableListOf<KaraokeSyllable>()
    syllables.forEach { syllable ->
        currentWord.add(syllable)
        if (syllable.content.trimEnd().length < syllable.content.length) {
            words.add(currentWord.toList())
            currentWord = mutableListOf()
        }
    }
    if (currentWord.isNotEmpty()) {
        words.add(currentWord.toList())
    }
    return words
}

fun measureSyllablesAndDetermineAnimation(
    syllables: List<KaraokeSyllable>,
    textMeasurer: TextMeasurer,
    style: TextStyle,
    phoneticStyle: TextStyle,
    isAccompanimentLine: Boolean,
    spaceWidth: Float,
    enableSyllableBounce: Boolean = true,
    enableIndicSyllableBounce: Boolean = true
): List<SyllableLayout> {
    val words = groupIntoWords(syllables)
    val fastCharAnimationThresholdMs = 200f

    return words.flatMapIndexed { wordIndex, word ->
        val wordContent = word.joinToString("") { it.content }
        val wordClusters = wordContent.getGraphemeClusters()
        val wordDuration = if (word.isNotEmpty()) word.last().end - word.first().start else 0
        val perCharDuration = if (wordClusters.isNotEmpty() && wordDuration > 0) {
            wordDuration.toFloat() / wordClusters.size
        } else {
            0f
        }

        val useAwesomeAnimation =
            perCharDuration > fastCharAnimationThresholdMs && wordDuration >= 1000
                    && !wordContent.shouldUseSimpleAnimation(
                        enableSyllableBounce = enableSyllableBounce,
                        enableIndicSyllableBounce = enableIndicSyllableBounce
                    )
                    && !isAccompanimentLine

        word.map { syllable ->
            val layoutResult = textMeasurer.measure(syllable.content, style)
            val phoneticLayout = if (!syllable.phonetic.isNullOrBlank()) {
                textMeasurer.measure(syllable.phonetic!!, phoneticStyle)
            } else null

            // --- Fix: 修正尾部空格宽度丢失 ---
            var layoutWidth = layoutResult.size.width.toFloat()
            if (syllable.content.endsWith(" ")) {
                val trimmedWidth = textMeasurer.measure(syllable.content.trimEnd(), style).size.width.toFloat()
                if (layoutWidth <= trimmedWidth) {
                    val spaceCount = syllable.content.length - syllable.content.trimEnd().length
                    layoutWidth = trimmedWidth + (spaceWidth * spaceCount)
                }
            }
            // -----------------------------

            // Ensure width is at least phonetic width
            val finalWidth = maxOf(layoutWidth, phoneticLayout?.size?.width?.toFloat() ?: 0f)

            // 如果需要高级动画，按字形簇（Grapheme Cluster）预先测量每个字符
            val (charLayouts, charBounds) = if (useAwesomeAnimation) {
                val clusters = syllable.content.getGraphemeClusters()
                val layouts = clusters.map { cluster ->
                    textMeasurer.measure(cluster.text, style)
                }
                val bounds = clusters.map { cluster ->
                    val boxes = (cluster.startIndex until cluster.endIndex).map { index ->
                        layoutResult.getBoundingBox(index)
                    }
                    Rect(
                        left = boxes.minOf { it.left },
                        top = boxes.minOf { it.top },
                        right = boxes.maxOf { it.right },
                        bottom = boxes.maxOf { it.bottom }
                    )
                }
                layouts to bounds
            } else {
                null to null
            }

            SyllableLayout(
                syllable = syllable,
                textLayoutResult = layoutResult,
                wordId = wordIndex,
                useAwesomeAnimation = useAwesomeAnimation,
                width = finalWidth, 
                charLayouts = charLayouts,      // 存入缓存
                charOriginalBounds = charBounds,
                firstBaseline = layoutResult.firstBaseline,
                phonetic = syllable.phonetic,
                phoneticLayoutResult = phoneticLayout
            )
        }
    }
}

fun calculateGreedyWrappedLines(
    syllableLayouts: List<SyllableLayout>,
    availableWidthPx: Float,
    textMeasurer: TextMeasurer,
    style: TextStyle
): List<WrappedLine> {
    val lines = mutableListOf<WrappedLine>()
    val currentLine = mutableListOf<SyllableLayout>()
    var currentLineWidth = 0f

    val wordGroups = mutableListOf<List<SyllableLayout>>()
    if (syllableLayouts.isNotEmpty()) {
        var currentWordGroup = mutableListOf<SyllableLayout>()
        var currentWordId = syllableLayouts.first().wordId

        syllableLayouts.forEach { layout ->
            if (layout.wordId != currentWordId) {
                wordGroups.add(currentWordGroup)
                currentWordGroup = mutableListOf()
                currentWordId = layout.wordId
            }
            currentWordGroup.add(layout)
        }
        wordGroups.add(currentWordGroup)
    }

    // 按单词进行排版
    wordGroups.forEach { wordSyllables ->
        val wordWidth = wordSyllables.sumOf { it.width.toDouble() }.toFloat()

        // 如果当前行能放下整个单词
        if (currentLineWidth + wordWidth <= availableWidthPx) {
            currentLine.addAll(wordSyllables)
            currentLineWidth += wordWidth
        } else {
            // 放不下，先换行（如果当前行不是空的）
            if (currentLine.isNotEmpty()) {
                val trimmedDisplayLine = trimDisplayLineTrailingSpaces(currentLine, textMeasurer, style)
                if (trimmedDisplayLine.syllables.isNotEmpty()) {
                    lines.add(trimmedDisplayLine)
                }
                currentLine.clear()
                currentLineWidth = 0f
            }

            // 换行后，检查新行能不能放下这个单词
            if (wordWidth <= availableWidthPx) {
                // 能放下，直接加入
                currentLine.addAll(wordSyllables)
                currentLineWidth += wordWidth
            } else {
                // 特殊情况: 单词超级长（比如德语符合词），比一整行还宽
                // 此时必须破坏单词完整性，退化回按音节换行
                wordSyllables.forEach { syllable ->
                    if (currentLineWidth + syllable.width > availableWidthPx && currentLine.isNotEmpty()) {
                        val trimmedLine = trimDisplayLineTrailingSpaces(currentLine, textMeasurer, style)
                        if (trimmedLine.syllables.isNotEmpty()) lines.add(trimmedLine)
                        currentLine.clear()
                        currentLineWidth = 0f
                    }
                    currentLine.add(syllable)
                    currentLineWidth += syllable.width
                }
            }
        }
    }

    // 处理最后一行
    if (currentLine.isNotEmpty()) {
        val trimmedDisplayLine = trimDisplayLineTrailingSpaces(currentLine, textMeasurer, style)
        if (trimmedDisplayLine.syllables.isNotEmpty()) {
            lines.add(trimmedDisplayLine)
        }
    }
    return lines
}

fun calculateBalancedLines(
    syllableLayouts: List<SyllableLayout>,
    availableWidthPx: Float,
    textMeasurer: TextMeasurer,
    style: TextStyle
): List<WrappedLine> {
    if (syllableLayouts.isEmpty()) return emptyList()

    val n = syllableLayouts.size
    val costs = DoubleArray(n + 1) { Double.POSITIVE_INFINITY }
    val breaks = IntArray(n + 1)
    costs[0] = 0.0

    for (i in 1..n) {
        var currentLineWidth = 0f
        for (j in i downTo 1) {
            if (j > 1 && syllableLayouts[j - 2].wordId == syllableLayouts[j - 1].wordId) {
                currentLineWidth += syllableLayouts[j - 1].width
                if (currentLineWidth > availableWidthPx) break
                continue
            }

            currentLineWidth += syllableLayouts[j - 1].width

            if (currentLineWidth > availableWidthPx) break

            val badness = (availableWidthPx - currentLineWidth).pow(2).toDouble()

            if (costs[j - 1] != Double.POSITIVE_INFINITY && costs[j - 1] + badness < costs[i]) {
                costs[i] = costs[j - 1] + badness
                breaks[i] = j - 1
            }
        }
    }

    if (costs[n] == Double.POSITIVE_INFINITY) {
        return calculateGreedyWrappedLines(syllableLayouts, availableWidthPx, textMeasurer, style)
    }

    val lines = mutableListOf<WrappedLine>()
    var currentIndex = n
    while (currentIndex > 0) {
        val startIndex = breaks[currentIndex]
        val lineSyllables = syllableLayouts.subList(startIndex, currentIndex)
        val trimmedLine = trimDisplayLineTrailingSpaces(lineSyllables, textMeasurer, style)
        lines.add(0, trimmedLine)
        currentIndex = startIndex
    }

    return lines
}

/**
 * 修复版：移除 Alignment 依赖，使用布尔值控制布局起始点，解决 RTL 居中问题
 */
fun calculateStaticLineLayout(
    wrappedLines: List<WrappedLine>,
    isLineRightAligned: Boolean,
    canvasWidth: Float,
    lineHeight: Float,
    phoneticHeight: Float,
    isRtl: Boolean
): List<List<SyllableLayout>> {
    val layoutsByWord = mutableMapOf<Int, MutableList<SyllableLayout>>()

    val hasPhoneticInBlock = wrappedLines.any { wl -> wl.syllables.any { it.phoneticLayoutResult != null } }

    val positionedLines = wrappedLines.mapIndexed { lineIndex, wrappedLine ->
        val maxBaselineInLine = wrappedLine.syllables.maxOfOrNull { it.firstBaseline } ?: 0f
        
        val rowHeight = lineHeight + if (hasPhoneticInBlock) (phoneticHeight * 0.7f) else 0f
        val rowTopY = lineIndex * rowHeight + if (hasPhoneticInBlock) (phoneticHeight * 0.7f) else 0f

        // 核心逻辑：如果是右对齐，起始点 = 画布宽 - 行宽。否则为 0。
        val startX = if (isLineRightAligned) {
            canvasWidth - wrappedLine.totalWidth
        } else {
            0f
        }

        // 下面的逻辑处理 RTL 单词内的排列，保持不变
        var currentX = if (isRtl) startX + wrappedLine.totalWidth else startX

        wrappedLine.syllables.map { initialLayout ->
            val positionX = if (isRtl) {
                currentX - initialLayout.width
            } else {
                currentX
            }
            val verticalOffset = maxBaselineInLine - initialLayout.firstBaseline
            val positionY = rowTopY + verticalOffset
            val positionedLayout = initialLayout.copy(position = Offset(positionX, positionY))
            layoutsByWord.getOrPut(positionedLayout.wordId) { mutableListOf() }
                .add(positionedLayout)

            if (isRtl) {
                currentX -= positionedLayout.width
            } else {
                currentX += positionedLayout.width
            }

            positionedLayout
        }
    }

    val animInfoByWord = mutableMapOf<Int, WordAnimationInfo>()
    val charOffsetsBySyllable = mutableMapOf<SyllableLayout, Int>()

    layoutsByWord.forEach { (wordId, layouts) ->
        if (layouts.first().useAwesomeAnimation) {
            animInfoByWord[wordId] = WordAnimationInfo(
                wordStartTime = layouts.minOf { it.syllable.start }.toLong(),
                wordEndTime = layouts.maxOf { it.syllable.end }.toLong(),
                wordContent = layouts.joinToString("") { it.syllable.content })
            var runningCharOffset = 0
            layouts.forEach { layout ->
                charOffsetsBySyllable[layout] = runningCharOffset
                runningCharOffset += layout.charLayouts?.size ?: layout.syllable.content.getGraphemeClusters().size
            }
        }
    }

    return positionedLines.map { line ->
        line.map { positionedLayout ->
            val wordLayouts = layoutsByWord.getValue(positionedLayout.wordId)
            val minX = wordLayouts.minOf { it.position.x }
            val maxX = wordLayouts.maxOf { it.position.x + it.width }
            val bottomY = wordLayouts.maxOf { it.position.y + it.textLayoutResult.size.height }

            positionedLayout.copy(
                wordPivot = Offset(x = (minX + maxX) / 2f, y = bottomY),
                wordAnimInfo = animInfoByWord[positionedLayout.wordId],
                charOffsetInWord = charOffsetsBySyllable[positionedLayout] ?: 0
            )
        }
    }
}

fun trimDisplayLineTrailingSpaces(
    displayLineSyllables: List<SyllableLayout>, textMeasurer: TextMeasurer, style: TextStyle
): WrappedLine {
    if (displayLineSyllables.isEmpty()) {
        return WrappedLine(emptyList(), 0f)
    }

    val processedSyllables = displayLineSyllables.toMutableList()
    var lastIndex = processedSyllables.lastIndex

    while (lastIndex >= 0 && processedSyllables[lastIndex].syllable.content.isBlank()) {
        processedSyllables.removeAt(lastIndex)
        lastIndex--
    }

    if (processedSyllables.isEmpty()) {
        return WrappedLine(emptyList(), 0f)
    }

    val lastLayout = processedSyllables.last()
    val originalContent = lastLayout.syllable.content
    val trimmedContent = originalContent.trimEnd()

    if (trimmedContent.length < originalContent.length) {
        if (trimmedContent.isNotEmpty()) {
            val trimmedLayoutResult = textMeasurer.measure(trimmedContent, style)
            val trimmedLayout = lastLayout.copy(
                syllable = lastLayout.syllable.copy(content = trimmedContent),
                textLayoutResult = trimmedLayoutResult,
                width = trimmedLayoutResult.size.width.toFloat()
            )
            processedSyllables[processedSyllables.lastIndex] = trimmedLayout
        } else {
            processedSyllables.removeAt(processedSyllables.lastIndex)
        }
    }

    val totalWidth = processedSyllables.sumOf { it.width.toDouble() }.toFloat()
    return WrappedLine(processedSyllables, totalWidth)
}
